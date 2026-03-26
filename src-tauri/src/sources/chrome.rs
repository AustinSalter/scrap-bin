//! Chrome Bookmark HTML Import
//!
//! Parses Chrome's Netscape-format HTML bookmark export and imports
//! bookmarks as fragments. Optionally fetches article content for each URL.

use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;
use thiserror::Error;
use tracing::info;
use ulid::Ulid;

use crate::chroma::client::get_client;
use crate::chroma::collections::{get_collection_id, COLLECTION_CHROME_BOOKMARKS};
use crate::content_extractor;
use crate::fragment::{self, Disposition, Fragment, SourceType};
use crate::grpc_client::get_grpc_client;

#[derive(Error, Debug)]
pub enum SourceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Chroma error: {0}")]
    Chroma(#[from] crate::chroma::client::ChromaError),
    #[error("gRPC error: {0}")]
    Grpc(#[from] crate::grpc_client::GrpcError),
}

impl Serialize for SourceError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// A bookmark parsed from Chrome's HTML export
#[derive(Debug, Clone)]
pub struct ChromeBookmark {
    pub url: String,
    pub title: String,
    pub folder_path: Vec<String>,
    pub date_added: Option<String>,
}

/// Result of importing Chrome bookmarks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromeImportResult {
    pub bookmarks_parsed: usize,
    pub imported: usize,
    pub skipped_duplicate: usize,
    pub content_fetched: usize,
    pub errors: Vec<String>,
}

/// Parse Chrome's Netscape bookmark HTML format.
///
/// Uses line-by-line parsing since the Netscape format uses non-standard
/// HTML that DOM parsers handle inconsistently. Tracks folder depth via
/// `<DL>` open/close tags and folder names via `<H3>` tags.
pub fn parse_chrome_bookmarks(html: &str) -> Vec<ChromeBookmark> {
    let mut bookmarks = Vec::new();
    let mut folder_stack: Vec<String> = Vec::new();

    for line in html.lines() {
        let trimmed = line.trim();

        // Folder open: <DT><H3 ...>Folder Name</H3>
        if let Some(h3_content) = extract_tag_content(trimmed, "h3") {
            folder_stack.push(h3_content);
            continue;
        }

        // Folder close: </DL>
        if trimmed.to_lowercase().starts_with("</dl>") {
            folder_stack.pop();
            continue;
        }

        // Bookmark: <DT><A HREF="..." ADD_DATE="...">Title</A>
        if let Some((url, title, add_date)) = extract_bookmark_link(trimmed) {
            if url.starts_with("http") {
                bookmarks.push(ChromeBookmark {
                    url,
                    title,
                    folder_path: folder_stack.clone(),
                    date_added: add_date,
                });
            }
        }
    }

    bookmarks
}

/// Extract text content between opening and closing tags (case-insensitive).
/// Returns None if the tag is not found.
fn extract_tag_content(line: &str, tag: &str) -> Option<String> {
    let lower = line.to_lowercase();
    let open_tag = format!("<{}", tag);
    let close_tag = format!("</{}>", tag);

    let open_pos = lower.find(&open_tag)?;
    // Find the end of the opening tag (the '>' after attributes)
    let tag_end = lower[open_pos..].find('>')? + open_pos + 1;
    let close_pos = lower.find(&close_tag)?;

    if tag_end <= close_pos {
        let content = line[tag_end..close_pos].trim().to_string();
        if !content.is_empty() {
            return Some(content);
        }
    }
    None
}

/// Extract bookmark URL, title, and add_date from an <A> tag line.
fn extract_bookmark_link(line: &str) -> Option<(String, String, Option<String>)> {
    let lower = line.to_lowercase();
    if !lower.contains("<a ") {
        return None;
    }

    // Extract HREF
    let href_pos = lower.find("href=\"")?;
    let href_start = href_pos + 6;
    let href_end = lower[href_start..].find('"')? + href_start;
    let url = line[href_start..href_end].to_string();

    // Extract ADD_DATE (optional)
    let add_date = lower.find("add_date=\"").and_then(|pos| {
        let start = pos + 10;
        let end = lower[start..].find('"')? + start;
        let ts_str = &line[start..end];
        ts_str.parse::<i64>().ok().and_then(|secs| {
            Utc.timestamp_opt(secs, 0)
                .single()
                .map(|dt| dt.to_rfc3339())
        })
    });

    // Extract title (text between > and </a>)
    let a_close = lower.find("</a>")?;
    let a_tag_end = lower[..a_close].rfind('>')? + 1;
    let title = line[a_tag_end..a_close].trim().to_string();

    Some((url, title, add_date))
}

/// Convert a Chrome bookmark to a Fragment (title-only content, no fetch)
fn bookmark_to_fragment(bookmark: &ChromeBookmark) -> Fragment {
    let content = if bookmark.title.is_empty() {
        bookmark.url.clone()
    } else {
        format!("{}\n{}", bookmark.title, bookmark.url)
    };

    let hash = fragment::content_hash(&content);
    let token_count = fragment::estimate_tokens(&content);
    let modified_at = bookmark
        .date_added
        .clone()
        .unwrap_or_else(|| Utc::now().to_rfc3339());

    Fragment {
        id: Ulid::new().to_string(),
        content,
        source_type: SourceType::ChromeBookmarks,
        source_path: bookmark.url.clone(),
        chunk_index: 0,
        heading_path: bookmark.folder_path.clone(),
        tags: Vec::new(),
        token_count,
        content_hash: hash,
        modified_at,
        cluster_id: None,
        disposition: Disposition::Inbox,
        highlights: vec![],
        metadata: serde_json::json!({
            "url": bookmark.url,
            "title": bookmark.title,
            "folder_path": bookmark.folder_path.join(" > "),
        }),
    }
}

/// Import Chrome bookmarks from an HTML export file.
///
/// If `fetch_content` is true, fetches article content for each URL
/// and creates richer fragments with the full article text.
#[tauri::command]
pub async fn source_chrome_import_bookmarks(
    path: String,
    fetch_content: bool,
) -> Result<ChromeImportResult, SourceError> {
    let canonical = tokio::fs::canonicalize(Path::new(&path)).await?;
    let meta = tokio::fs::metadata(&canonical).await?;
    if !meta.is_file() {
        return Err(SourceError::Parse("Path is not a regular file".to_string()));
    }
    let html = tokio::fs::read_to_string(&canonical).await?;
    let bookmarks = parse_chrome_bookmarks(&html);
    let total_parsed = bookmarks.len();
    info!(count = total_parsed, "Parsed Chrome bookmarks");

    if bookmarks.is_empty() {
        return Ok(ChromeImportResult {
            bookmarks_parsed: 0,
            imported: 0,
            skipped_duplicate: 0,
            content_fetched: 0,
            errors: Vec::new(),
        });
    }

    // Get existing URLs for dedup
    let chroma_client = get_client();
    let coll_id = get_collection_id(COLLECTION_CHROME_BOOKMARKS).await?;
    let mut existing_urls = HashSet::new();

    let existing_result = chroma_client
        .get(
            &coll_id,
            None,
            None,
            Some(vec!["metadatas".to_string()]),
            None,
            None,
        )
        .await;
    if let Ok(result) = existing_result {
        if let Some(metas) = &result.metadatas {
            for meta in metas.iter().flatten() {
                if let Some(url) = meta.get("url").and_then(|v| v.as_str()) {
                    existing_urls.insert(url.to_string());
                }
            }
        }
    }

    let mut fragments = Vec::new();
    let mut skipped = 0usize;
    let mut content_fetched = 0usize;
    let mut errors = Vec::new();

    for bookmark in &bookmarks {
        // Dedup by URL
        if existing_urls.contains(&bookmark.url) {
            skipped += 1;
            continue;
        }
        existing_urls.insert(bookmark.url.clone()); // prevent dupes within batch

        if fetch_content {
            // Rate limit: 1 req/sec
            tokio::time::sleep(Duration::from_secs(1)).await;

            match content_extractor::fetch_article_content(&bookmark.url).await {
                Some(article) => {
                    content_fetched += 1;
                    // Create fragment(s) from fetched article content
                    let source_path = bookmark.url.clone();
                    let chunks = crate::chunker::chunk_plain_text(&article.text, &source_path);

                    for chunk in &chunks {
                        let hash = fragment::content_hash(&chunk.content);
                        let modified_at = bookmark
                            .date_added
                            .clone()
                            .unwrap_or_else(|| Utc::now().to_rfc3339());

                        fragments.push(Fragment {
                            id: Ulid::new().to_string(),
                            content: chunk.content.clone(),
                            source_type: SourceType::ChromeBookmarks,
                            source_path: source_path.clone(),
                            chunk_index: chunk.chunk_index,
                            heading_path: bookmark.folder_path.clone(),
                            tags: Vec::new(),
                            token_count: chunk.token_count,
                            content_hash: hash,
                            modified_at,
                            cluster_id: None,
                            disposition: Disposition::Inbox,
                            highlights: vec![],
                            metadata: serde_json::json!({
                                "url": bookmark.url,
                                "title": article.title.as_deref().unwrap_or(&bookmark.title),
                                "folder_path": bookmark.folder_path.join(" > "),
                                "word_count": article.word_count,
                                "content_fetched": true,
                            }),
                        });
                    }
                }
                None => {
                    // Fallback: title-only fragment
                    fragments.push(bookmark_to_fragment(bookmark));
                }
            }
        } else {
            fragments.push(bookmark_to_fragment(bookmark));
        }
    }

    let imported = fragments.len();

    // Embed and store in batches
    if !fragments.is_empty() {
        let grpc = get_grpc_client()?;

        // Process in batches of 50
        for batch in fragments.chunks(50) {
            let texts: Vec<String> = batch.iter().map(|f| f.content.clone()).collect();
            let embeddings = match grpc.embed_batch(texts).await {
                Ok(e) => e,
                Err(e) => {
                    errors.push(format!("Embedding batch failed: {}", e));
                    continue;
                }
            };

            let ids: Vec<String> = batch.iter().map(|f| f.id.clone()).collect();
            let documents: Vec<String> = batch.iter().map(|f| f.content.clone()).collect();
            let metadatas: Vec<serde_json::Value> = batch
                .iter()
                .map(fragment::fragment_to_chroma_metadata)
                .collect();

            if let Err(e) = chroma_client
                .add(&coll_id, ids, Some(embeddings), Some(documents), Some(metadatas))
                .await
            {
                errors.push(format!("Chroma add failed: {}", e));
            }
        }

        info!(imported = imported, "Stored Chrome bookmark fragments in Chroma");
    }

    Ok(ChromeImportResult {
        bookmarks_parsed: total_parsed,
        imported,
        skipped_duplicate: skipped,
        content_fetched,
        errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_chrome_bookmarks_basic() {
        let html = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
<META HTTP-EQUIV="Content-Type" CONTENT="text/html; charset=UTF-8">
<TITLE>Bookmarks</TITLE>
<H1>Bookmarks</H1>
<DL><p>
    <DT><H3>Bookmarks Bar</H3>
    <DL><p>
        <DT><A HREF="https://example.com" ADD_DATE="1700000000">Example</A>
        <DT><H3>Dev</H3>
        <DL><p>
            <DT><A HREF="https://rust-lang.org" ADD_DATE="1700000100">Rust</A>
        </DL><p>
    </DL><p>
</DL><p>"#;

        let bookmarks = parse_chrome_bookmarks(html);
        assert!(bookmarks.len() >= 2);

        let example = bookmarks.iter().find(|b| b.url.contains("example.com"));
        assert!(example.is_some());
        let example = example.unwrap();
        assert_eq!(example.title, "Example");

        let rust = bookmarks.iter().find(|b| b.url.contains("rust-lang"));
        assert!(rust.is_some());
        let rust = rust.unwrap();
        assert_eq!(rust.title, "Rust");
        assert!(rust.folder_path.contains(&"Dev".to_string()));
    }

    #[test]
    fn test_bookmark_to_fragment() {
        let bookmark = ChromeBookmark {
            url: "https://example.com/article".to_string(),
            title: "Test Article".to_string(),
            folder_path: vec!["Bookmarks Bar".to_string(), "Reading".to_string()],
            date_added: Some("2025-01-15T10:30:00Z".to_string()),
        };

        let frag = bookmark_to_fragment(&bookmark);
        assert_eq!(frag.source_type, SourceType::ChromeBookmarks);
        assert!(frag.content.contains("Test Article"));
        assert!(frag.content.contains("https://example.com/article"));
        assert_eq!(frag.heading_path, vec!["Bookmarks Bar", "Reading"]);
    }
}
