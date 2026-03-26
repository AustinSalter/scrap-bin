use crate::chroma::client::{get_client, ChromaError};
use crate::chroma::collections::{get_collection_id, COLLECTION_RSS};
use crate::config::{self, SourceConfig};
use crate::fragment::{self, Fragment, SourceType};
use crate::grpc_client::{get_grpc_client, GrpcError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum SourceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Config error: {0}")]
    Config(#[from] config::ConfigError),
    #[error("Feed parse error: {0}")]
    FeedParse(String),
    #[error("Chroma error: {0}")]
    Chroma(#[from] ChromaError),
    #[error("gRPC error: {0}")]
    Grpc(#[from] GrpcError),
}

impl Serialize for SourceError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<reqwest::Error> for SourceError {
    fn from(err: reqwest::Error) -> Self {
        SourceError::Http(err.to_string())
    }
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssAddFeedResult {
    pub source_id: String,
    pub feed_title: String,
    pub feed_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssPollResult {
    pub imported: usize,
    pub skipped: usize,
    pub entries_fetched: usize,
    pub feed_title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssCheckResult {
    pub reachable: bool,
    pub feed_title: Option<String>,
    pub entry_count: usize,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Normalizes a feed URL. For Substack sites, appends `/feed` if not present.
pub fn normalize_feed_url(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');

    if url.contains("substack.com") && !url.ends_with("/feed") {
        return format!("{}/feed", url);
    }

    url.to_string()
}

/// Fetches and parses a feed from a URL, returning the parsed feed model.
async fn fetch_and_parse_feed(url: &str) -> Result<feed_rs::model::Feed, SourceError> {
    let client = reqwest::Client::builder()
        .user_agent("Scrapbin/0.1")
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let resp = client.get(url).send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(SourceError::Http(format!(
            "Feed fetch returned {}: {}",
            status, body
        )));
    }

    let bytes = resp.bytes().await?;
    feed_rs::parser::parse(&bytes[..])
        .map_err(|e| SourceError::FeedParse(e.to_string()))
}

/// Loads the last sync timestamp for an RSS source.
fn load_last_sync(source_id: &str) -> Option<String> {
    let path = config::app_data_dir()
        .ok()?
        .join(format!("rss_sync_{}.json", source_id));
    let data = std::fs::read_to_string(path).ok()?;
    let obj: serde_json::Value = serde_json::from_str(&data).ok()?;
    obj.get("last_sync")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Saves the last sync timestamp for an RSS source.
fn save_last_sync(source_id: &str, timestamp: &str) -> Result<(), SourceError> {
    let path = config::app_data_dir()?.join(format!("rss_sync_{}.json", source_id));
    let obj = serde_json::json!({ "last_sync": timestamp });
    std::fs::write(path, serde_json::to_string_pretty(&obj)?)?;
    Ok(())
}

/// Extracts the best available content from a feed entry.
fn entry_content(entry: &feed_rs::model::Entry) -> String {
    // Prefer full content over summary.
    if let Some(ref content) = entry.content {
        if let Some(ref body) = content.body {
            if !body.trim().is_empty() {
                return body.clone();
            }
        }
    }
    if let Some(ref summary) = entry.summary {
        if !summary.content.trim().is_empty() {
            return summary.content.clone();
        }
    }
    String::new()
}

/// Converts a feed entry into one or more Fragments (chunked if long).
fn entry_to_fragments(
    entry: &feed_rs::model::Entry,
    feed_url: &str,
    feed_title: &str,
) -> Vec<Fragment> {
    let title = entry
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_default();
    let body = entry_content(entry);

    if body.trim().is_empty() && title.trim().is_empty() {
        return Vec::new();
    }

    // Combine title + body for chunking.
    let full_text = if title.is_empty() {
        body.clone()
    } else if body.is_empty() {
        title.clone()
    } else {
        format!("{}\n\n{}", title, body)
    };

    let entry_url = entry
        .links
        .first()
        .map(|l| l.href.clone())
        .unwrap_or_default();

    let author = entry
        .authors
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_default();

    let published_at = entry
        .published
        .or(entry.updated)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let source_path = if entry_url.is_empty() {
        format!("rss://{}", feed_url)
    } else {
        entry_url.clone()
    };

    // Chunk the content.
    use crate::chunker;
    let chunks = chunker::chunk_plain_text(&full_text, &source_path);

    chunks
        .into_iter()
        .enumerate()
        .map(|(idx, chunk)| {
            let hash = fragment::content_hash(&chunk.content);
            let token_count = fragment::estimate_tokens(&chunk.content);
            let id = ulid::Ulid::new().to_string();

            Fragment {
                id,
                content: chunk.content,
                source_type: SourceType::Rss,
                source_path: source_path.clone(),
                chunk_index: idx,
                heading_path: Vec::new(),
                tags: Vec::new(),
                token_count,
                content_hash: hash,
                modified_at: published_at.clone(),
                cluster_id: None,
                disposition: fragment::Disposition::Inbox,
                highlights: vec![],
                metadata: serde_json::json!({
                    "feed_url": feed_url,
                    "feed_title": feed_title,
                    "entry_url": entry_url,
                    "entry_title": title,
                    "author": author,
                    "published_at": published_at,
                }),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Adds a new RSS feed: normalizes the URL, fetches to validate, creates a
/// SourceConfig, and saves it.
#[tauri::command]
pub async fn source_rss_add_feed(url: String) -> Result<RssAddFeedResult, SourceError> {
    let feed_url = normalize_feed_url(&url);

    // Validate by fetching and parsing.
    let feed = fetch_and_parse_feed(&feed_url).await?;

    let feed_title = feed
        .title
        .map(|t| t.content)
        .unwrap_or_else(|| "Untitled Feed".to_string());

    let source_id = ulid::Ulid::new().to_string();

    let source = SourceConfig {
        id: source_id.clone(),
        source_type: SourceType::Rss,
        display_name: feed_title.clone(),
        config: serde_json::json!({
            "feed_url": feed_url,
            "feed_title": feed_title,
        }),
        default_disposition: fragment::Disposition::Inbox,
        sync_schedule: None,
        enabled: true,
        vault_subfolder: None,
    };

    let mut sources = config::load_sources()?;
    sources.push(source);
    config::save_sources(&sources)?;

    tracing::info!("Added RSS feed: {} ({})", feed_title, feed_url);

    Ok(RssAddFeedResult {
        source_id,
        feed_title,
        feed_url,
    })
}

/// Polls a single RSS feed source: fetches new entries since last sync,
/// converts to fragments, embeds, and stores in Chroma.
#[tauri::command]
pub async fn source_rss_poll(source_id: String) -> Result<RssPollResult, SourceError> {
    let sources = config::load_sources()?;
    let source = sources
        .iter()
        .find(|s| s.id == source_id)
        .ok_or_else(|| SourceError::Config(config::ConfigError::SourceNotFound(source_id.clone())))?;

    let feed_url = source
        .config
        .get("feed_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let feed_title = source
        .config
        .get("feed_title")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown Feed")
        .to_string();

    if feed_url.is_empty() {
        return Err(SourceError::FeedParse("No feed_url configured".to_string()));
    }

    let feed = fetch_and_parse_feed(&feed_url).await?;
    let last_sync = load_last_sync(&source_id);

    // Parse last_sync timestamp for filtering.
    let last_sync_dt = last_sync.as_ref().and_then(|ts| {
        chrono::DateTime::parse_from_rfc3339(ts)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    });

    // Filter entries newer than last sync.
    let entries: Vec<&feed_rs::model::Entry> = feed
        .entries
        .iter()
        .filter(|e| {
            if let Some(ref cutoff) = last_sync_dt {
                let entry_dt = e.published.or(e.updated);
                match entry_dt {
                    Some(dt) => dt > *cutoff,
                    None => true, // Include entries with no date
                }
            } else {
                true // No previous sync — import all
            }
        })
        .collect();

    let entries_fetched = entries.len();

    tracing::info!(
        "RSS {}: {} entries to process (since {:?})",
        feed_title,
        entries_fetched,
        last_sync
    );

    // Convert entries to fragments.
    let mut all_fragments: Vec<Fragment> = Vec::new();
    for entry in &entries {
        let frags = entry_to_fragments(entry, &feed_url, &feed_title);
        all_fragments.extend(frags);
    }

    // Dedup within the batch by content_hash.
    let mut seen_hashes = std::collections::HashSet::new();
    all_fragments.retain(|f| seen_hashes.insert(f.content_hash.clone()));

    // Dedup against existing Chroma collection (mirrors the Twitter pattern).
    let client = get_client();
    let coll_id = get_collection_id(COLLECTION_RSS).await?;
    let existing_result = client
        .get(&coll_id, None, None, Some(vec!["metadatas".to_string()]), None, None)
        .await;
    if let Ok(result) = existing_result {
        if let Some(metas) = &result.metadatas {
            let existing_hashes: std::collections::HashSet<String> = metas
                .iter()
                .flatten()
                .filter_map(|m| m.get("content_hash").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .collect();
            all_fragments.retain(|f| !existing_hashes.contains(&f.content_hash));
        }
    }

    let skipped = entries_fetched - all_fragments.len();
    let imported = all_fragments.len();

    // Embed and store.
    if !all_fragments.is_empty() {
        let grpc = get_grpc_client()?;

        let texts: Vec<String> = all_fragments.iter().map(|f| f.content.clone()).collect();
        let embeddings = grpc.embed_batch(texts).await?;

        let ids: Vec<String> = all_fragments.iter().map(|f| f.id.clone()).collect();
        let documents: Vec<String> = all_fragments.iter().map(|f| f.content.clone()).collect();
        let metadatas: Vec<serde_json::Value> = all_fragments
            .iter()
            .map(fragment::fragment_to_chroma_metadata)
            .collect();

        client
            .add(&coll_id, ids, Some(embeddings), Some(documents), Some(metadatas))
            .await?;

        tracing::info!("Stored {} RSS fragments in Chroma", imported);
    }

    // Update sync timestamp.
    let now = chrono::Utc::now().to_rfc3339();
    if let Err(e) = save_last_sync(&source_id, &now) {
        tracing::warn!("Failed to save RSS sync timestamp: {}", e);
    }

    Ok(RssPollResult {
        imported,
        skipped,
        entries_fetched,
        feed_title,
    })
}

/// Polls all enabled RSS feed sources, aggregating results.
#[tauri::command]
pub async fn source_rss_poll_all() -> Result<Vec<RssPollResult>, SourceError> {
    let sources = config::load_sources()?;
    let rss_sources: Vec<&SourceConfig> = sources
        .iter()
        .filter(|s| s.source_type == SourceType::Rss && s.enabled)
        .collect();

    let mut results = Vec::new();

    for source in rss_sources {
        match source_rss_poll(source.id.clone()).await {
            Ok(result) => results.push(result),
            Err(e) => {
                tracing::warn!("Failed to poll RSS feed {}: {}", source.display_name, e);
                results.push(RssPollResult {
                    imported: 0,
                    skipped: 0,
                    entries_fetched: 0,
                    feed_title: source.display_name.clone(),
                });
            }
        }
    }

    Ok(results)
}

/// Checks connectivity for an RSS feed source.
#[tauri::command]
pub async fn source_rss_check_connection(
    source_id: String,
) -> Result<RssCheckResult, SourceError> {
    let sources = config::load_sources()?;
    let source = sources
        .iter()
        .find(|s| s.id == source_id)
        .ok_or_else(|| SourceError::Config(config::ConfigError::SourceNotFound(source_id.clone())))?;

    let feed_url = source
        .config
        .get("feed_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if feed_url.is_empty() {
        return Ok(RssCheckResult {
            reachable: false,
            feed_title: None,
            entry_count: 0,
        });
    }

    match fetch_and_parse_feed(&feed_url).await {
        Ok(feed) => {
            let title = feed.title.map(|t| t.content);
            Ok(RssCheckResult {
                reachable: true,
                feed_title: title,
                entry_count: feed.entries.len(),
            })
        }
        Err(_) => Ok(RssCheckResult {
            reachable: false,
            feed_title: None,
            entry_count: 0,
        }),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_feed_url_substack() {
        assert_eq!(
            normalize_feed_url("https://example.substack.com"),
            "https://example.substack.com/feed"
        );
        assert_eq!(
            normalize_feed_url("https://example.substack.com/"),
            "https://example.substack.com/feed"
        );
        // Already has /feed — don't double-append.
        assert_eq!(
            normalize_feed_url("https://example.substack.com/feed"),
            "https://example.substack.com/feed"
        );
    }

    #[test]
    fn test_normalize_feed_url_non_substack() {
        assert_eq!(
            normalize_feed_url("https://blog.example.com/rss"),
            "https://blog.example.com/rss"
        );
        assert_eq!(
            normalize_feed_url("  https://example.com/feed.xml  "),
            "https://example.com/feed.xml"
        );
    }

    /// Parse a minimal Atom feed XML and return the first entry.
    fn parse_test_feed(xml: &str) -> feed_rs::model::Feed {
        feed_rs::parser::parse(xml.as_bytes()).expect("test feed should parse")
    }

    #[test]
    fn test_entry_to_fragments_basic() {
        let feed = parse_test_feed(r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Test Feed</title>
  <entry>
    <id>entry-1</id>
    <title>Test Article</title>
    <content type="text">This is the body of the article.</content>
    <link href="https://example.com/article"/>
    <author><name>Author Name</name></author>
    <published>2025-06-01T12:00:00Z</published>
  </entry>
</feed>"#);

        let entry = &feed.entries[0];
        let frags = entry_to_fragments(entry, "https://example.com/feed", "Test Feed");
        assert!(!frags.is_empty());

        let f = &frags[0];
        assert_eq!(f.source_type, SourceType::Rss);
        assert_eq!(f.source_path, "https://example.com/article");
        assert_eq!(f.disposition, fragment::Disposition::Inbox);
        assert_eq!(f.metadata["feed_url"], "https://example.com/feed");
        assert_eq!(f.metadata["feed_title"], "Test Feed");
        assert_eq!(f.metadata["author"], "Author Name");
        assert_eq!(f.metadata["entry_title"], "Test Article");
        assert!(f.content.contains("Test Article"));
        assert!(f.content.contains("body of the article"));
    }

    #[test]
    fn test_entry_to_fragments_long_content() {
        let word = "lorem ";
        let big_body = word.repeat(1000);

        let xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Test Feed</title>
  <entry>
    <id>entry-long</id>
    <title>Long Article</title>
    <content type="text">{}</content>
  </entry>
</feed>"#, big_body);

        let feed = parse_test_feed(&xml);
        let entry = &feed.entries[0];
        let frags = entry_to_fragments(entry, "https://example.com/feed", "Test Feed");
        assert!(frags.len() > 1, "Long content should produce multiple chunks");

        for (i, f) in frags.iter().enumerate() {
            assert_eq!(f.chunk_index, i);
        }
    }

    #[test]
    fn test_entry_to_fragments_empty() {
        let entry = feed_rs::model::Entry::default();

        let frags = entry_to_fragments(&entry, "https://example.com/feed", "Test");
        assert!(frags.is_empty());
    }
}
