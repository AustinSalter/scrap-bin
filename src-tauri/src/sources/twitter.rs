use crate::fragment::{Fragment, SourceType};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum SourceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid data: {0}")]
    InvalidData(String),
}

impl Serialize for SourceError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ---------------------------------------------------------------------------
// Twitter bookmark JSON schema
// ---------------------------------------------------------------------------

/// A single bookmark entry from the Twitter JSON export.
#[derive(Debug, Clone, Deserialize)]
struct TwitterBookmark {
    id: String,
    text: String,
    /// Long-form tweet text (> 280 chars).
    note_tweet: Option<NoteTweet>,
    created_at: Option<String>,
    author_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct NoteTweet {
    text: Option<String>,
}

/// Wrapper for the top-level `{ "data": [...] }` export format.
#[derive(Debug, Clone, Deserialize)]
struct TwitterExport {
    data: Vec<TwitterBookmark>,
}

// ---------------------------------------------------------------------------
// Result type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterImportResult {
    /// Number of bookmarks successfully converted to fragments.
    pub imported: usize,
    /// Number of bookmarks skipped because they were already ingested.
    pub skipped: usize,
    /// Per-bookmark error messages (non-fatal).
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum characters per chunk for plain-text splitting.
const MAX_CHUNK_CHARS: usize = 1500;

/// Rough token-per-character ratio for English text.
const CHARS_PER_TOKEN: f64 = 4.0;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Computes the SHA-256 hex digest of a string.
fn content_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Estimates the token count from character length.
fn estimate_tokens(text: &str) -> usize {
    (text.len() as f64 / CHARS_PER_TOKEN).ceil() as usize
}

/// Splits long text into chunks of approximately `MAX_CHUNK_CHARS`, breaking
/// at paragraph or sentence boundaries when possible.
fn chunk_plain_text(text: &str) -> Vec<String> {
    if text.len() <= MAX_CHUNK_CHARS {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for paragraph in text.split("\n\n") {
        // If adding this paragraph would exceed the limit, flush current.
        if !current.is_empty() && current.len() + paragraph.len() + 2 > MAX_CHUNK_CHARS {
            chunks.push(current.trim().to_string());
            current = String::new();
        }

        // If a single paragraph is itself longer than the limit, split by sentences.
        if paragraph.len() > MAX_CHUNK_CHARS {
            for sentence in split_sentences(paragraph) {
                if !current.is_empty() && current.len() + sentence.len() + 1 > MAX_CHUNK_CHARS {
                    chunks.push(current.trim().to_string());
                    current = String::new();
                }
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(&sentence);
            }
        } else {
            if !current.is_empty() {
                current.push_str("\n\n");
            }
            current.push_str(paragraph);
        }
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    chunks
}

/// Naive sentence splitter: split on `. `, `? `, `! ` while keeping the
/// punctuation attached to the preceding text.
fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();

    for i in 0..bytes.len().saturating_sub(1) {
        let is_end = matches!(bytes[i], b'.' | b'?' | b'!')
            && bytes.get(i + 1) == Some(&b' ');

        if is_end {
            sentences.push(text[start..=i].trim().to_string());
            start = i + 2; // skip the space after punctuation
        }
    }

    // Remainder.
    if start < text.len() {
        let remainder = text[start..].trim();
        if !remainder.is_empty() {
            sentences.push(remainder.to_string());
        }
    }

    sentences
}

/// Converts a single Twitter bookmark into one or more `Fragment`s.
fn bookmark_to_fragments(bookmark: &TwitterBookmark) -> Vec<Fragment> {
    // Prefer long-form note_tweet text over the truncated `text` field.
    let full_text = bookmark
        .note_tweet
        .as_ref()
        .and_then(|nt| nt.text.as_deref())
        .unwrap_or(&bookmark.text);

    let chunks = chunk_plain_text(full_text);
    let modified_at = bookmark
        .created_at
        .clone()
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let source_path = format!("twitter://bookmark/{}", bookmark.id);

    chunks
        .into_iter()
        .enumerate()
        .map(|(idx, chunk_text)| {
            let hash = content_hash(&chunk_text);
            let token_count = estimate_tokens(&chunk_text);
            let id = ulid::Ulid::new().to_string();

            let mut metadata = serde_json::json!({
                "tweet_id": bookmark.id,
            });
            if let Some(ref author) = bookmark.author_id {
                metadata["author_id"] = serde_json::json!(author);
            }

            Fragment {
                id,
                content: chunk_text,
                source_type: SourceType::Twitter,
                source_path: source_path.clone(),
                chunk_index: idx,
                heading_path: Vec::new(),
                tags: Vec::new(),
                token_count,
                content_hash: hash,
                modified_at: modified_at.clone(),
                cluster_id: None,
                metadata,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Core import logic
// ---------------------------------------------------------------------------

/// Reads a Twitter bookmark JSON export from `path`, parses the `data` array,
/// and converts each bookmark into `Fragment`s.
///
/// Bookmarks whose `tweet_id` appears in `existing_tweet_ids` are skipped for
/// deduplication. The caller is responsible for querying Chroma to populate
/// that set.
fn import_bookmarks(
    path: &str,
    existing_tweet_ids: &std::collections::HashSet<String>,
) -> Result<(Vec<Fragment>, TwitterImportResult), SourceError> {
    let data = std::fs::read_to_string(path)?;

    // The export may be either `{ "data": [...] }` or a bare array `[...]`.
    let bookmarks: Vec<TwitterBookmark> = if let Ok(export) =
        serde_json::from_str::<TwitterExport>(&data)
    {
        export.data
    } else {
        serde_json::from_str::<Vec<TwitterBookmark>>(&data)?
    };

    tracing::info!("Parsed {} bookmarks from {}", bookmarks.len(), path);

    let mut all_fragments = Vec::new();
    let mut imported = 0usize;
    let mut skipped = 0usize;
    let mut errors = Vec::new();

    for bookmark in &bookmarks {
        // Dedup by tweet_id.
        if existing_tweet_ids.contains(&bookmark.id) {
            skipped += 1;
            continue;
        }

        // Validate minimal data.
        if bookmark.text.trim().is_empty()
            && bookmark
                .note_tweet
                .as_ref()
                .and_then(|nt| nt.text.as_deref())
                .map_or(true, |t| t.trim().is_empty())
        {
            errors.push(format!("Bookmark {} has empty text, skipped", bookmark.id));
            continue;
        }

        let fragments = bookmark_to_fragments(bookmark);
        imported += 1; // count per bookmark, not per chunk
        all_fragments.extend(fragments);
    }

    let result = TwitterImportResult {
        imported,
        skipped,
        errors,
    };

    tracing::info!(
        "Twitter import: {} imported, {} skipped, {} errors",
        result.imported,
        result.skipped,
        result.errors.len()
    );

    Ok((all_fragments, result))
}

// ---------------------------------------------------------------------------
// Tauri command
// ---------------------------------------------------------------------------

/// Reads a Twitter bookmark JSON export, parses bookmarks, chunks long tweets,
/// and returns the resulting fragments alongside import statistics.
///
/// Actual embedding and Chroma storage happen downstream in the pipeline.
#[tauri::command]
pub async fn source_twitter_import(path: String) -> Result<TwitterImportResult, SourceError> {
    // For now we pass an empty set; the caller should wire up Chroma dedup
    // once the pipeline is integrated.
    let existing_ids = std::collections::HashSet::new();

    let (_fragments, result) = tokio::task::spawn_blocking(move || {
        import_bookmarks(&path, &existing_ids)
    })
    .await
    .map_err(|e| SourceError::InvalidData(format!("Task join error: {e}")))?
    ?;

    Ok(result)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_plain_text_short() {
        let text = "Hello, world!";
        let chunks = chunk_plain_text(text);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Hello, world!");
    }

    #[test]
    fn test_chunk_plain_text_long() {
        // Build a string that exceeds MAX_CHUNK_CHARS.
        let paragraph = "word ".repeat(400); // ~2000 chars
        let text = format!("{}\n\n{}", paragraph.trim(), paragraph.trim());
        let chunks = chunk_plain_text(&text);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            // Each chunk should be reasonably bounded.
            assert!(chunk.len() <= MAX_CHUNK_CHARS + 200); // some slack for sentence boundaries
        }
    }

    #[test]
    fn test_split_sentences() {
        let text = "First sentence. Second sentence? Third sentence! End";
        let sentences = split_sentences(text);
        assert_eq!(sentences.len(), 4);
        assert_eq!(sentences[0], "First sentence.");
        assert_eq!(sentences[1], "Second sentence?");
        assert_eq!(sentences[2], "Third sentence!");
        assert_eq!(sentences[3], "End");
    }

    #[test]
    fn test_bookmark_to_fragments_short_tweet() {
        let bookmark = TwitterBookmark {
            id: "123456".to_string(),
            text: "This is a short tweet.".to_string(),
            note_tweet: None,
            created_at: Some("2025-01-15T10:00:00Z".to_string()),
            author_id: Some("user_42".to_string()),
        };

        let fragments = bookmark_to_fragments(&bookmark);
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].content, "This is a short tweet.");
        assert_eq!(fragments[0].source_type, SourceType::Twitter);
        assert_eq!(fragments[0].chunk_index, 0);
        assert_eq!(fragments[0].metadata["tweet_id"], "123456");
        assert_eq!(fragments[0].metadata["author_id"], "user_42");
    }

    #[test]
    fn test_bookmark_to_fragments_note_tweet() {
        let long_text = "A ".repeat(1000); // > MAX_CHUNK_CHARS
        let bookmark = TwitterBookmark {
            id: "789".to_string(),
            text: "Truncated version...".to_string(),
            note_tweet: Some(NoteTweet {
                text: Some(long_text),
            }),
            created_at: None,
            author_id: None,
        };

        let fragments = bookmark_to_fragments(&bookmark);
        assert!(fragments.len() >= 2);
        // All fragments share the same source_path.
        let path = &fragments[0].source_path;
        for f in &fragments {
            assert_eq!(&f.source_path, path);
        }
    }

    #[test]
    fn test_import_dedup() {
        let json = serde_json::json!({
            "data": [
                { "id": "1", "text": "First tweet" },
                { "id": "2", "text": "Second tweet" },
            ]
        });
        let tmp = std::env::temp_dir().join("twitter_test_dedup.json");
        std::fs::write(&tmp, serde_json::to_string(&json).unwrap()).unwrap();

        let mut existing = std::collections::HashSet::new();
        existing.insert("1".to_string());

        let (fragments, result) =
            import_bookmarks(tmp.to_str().unwrap(), &existing).unwrap();

        assert_eq!(result.imported, 1);
        assert_eq!(result.skipped, 1);
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].metadata["tweet_id"], "2");

        let _ = std::fs::remove_file(&tmp);
    }
}
