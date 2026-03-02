use crate::chroma::client::{get_client, ChromaError};
use crate::chroma::collections::{get_collection_id, COLLECTION_READWISE};
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
    Config(#[from] crate::config::ConfigError),
    #[error("API key not configured")]
    ApiKeyMissing,
    #[error("Invalid data: {0}")]
    InvalidData(String),
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
// Readwise API v2 response types
// ---------------------------------------------------------------------------

/// Paginated response from `GET /api/v2/highlights/`.
#[derive(Debug, Clone, Deserialize)]
struct HighlightsResponse {
    count: usize,
    next: Option<String>,
    results: Vec<ReadwiseHighlight>,
}

/// A single highlight from the Readwise API.
#[derive(Debug, Clone, Deserialize)]
struct ReadwiseHighlight {
    id: u64,
    text: String,
    title: Option<String>,
    author: Option<String>,
    url: Option<String>,
    highlighted_at: Option<String>,
    book_id: Option<u64>,
    #[serde(default)]
    tags: Vec<ReadwiseTag>,
}

#[derive(Debug, Clone, Deserialize)]
struct ReadwiseTag {
    name: String,
}

// ---------------------------------------------------------------------------
// Result type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadwiseImportResult {
    /// Number of highlights converted to fragments.
    pub imported: usize,
    /// Total highlights fetched from the API (before dedup or filtering).
    pub total_fetched: usize,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const READWISE_API_BASE: &str = "https://readwise.io/api/v2";

/// Config key for persisting the last sync timestamp.
const LAST_SYNC_KEY: &str = "readwise_last_sync";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// Use shared helpers from fragment module.

/// Loads the Readwise API key from config, returning an error if not set.
fn get_api_key() -> Result<String, SourceError> {
    let config = crate::config::load_config()?;
    config
        .readwise_api_key
        .filter(|k| !k.trim().is_empty())
        .ok_or(SourceError::ApiKeyMissing)
}

/// Loads the last successful sync timestamp from config metadata, if any.
fn get_last_sync_timestamp() -> Option<String> {
    // We store the timestamp in a separate small JSON file alongside config.
    let path = crate::config::app_data_dir().ok()?.join("readwise_sync.json");
    let data = std::fs::read_to_string(path).ok()?;
    let obj: serde_json::Value = serde_json::from_str(&data).ok()?;
    obj.get(LAST_SYNC_KEY)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Persists the last sync timestamp.
fn save_last_sync_timestamp(timestamp: &str) -> Result<(), SourceError> {
    let path = crate::config::app_data_dir()?.join("readwise_sync.json");
    let obj = serde_json::json!({ LAST_SYNC_KEY: timestamp });
    std::fs::write(path, serde_json::to_string_pretty(&obj)?)?;
    Ok(())
}

/// Converts a Readwise highlight into a single `Fragment`.
fn highlight_to_fragment(h: &ReadwiseHighlight) -> Fragment {
    let hash = fragment::content_hash(&h.text);
    let token_count = fragment::estimate_tokens(&h.text);
    let id = ulid::Ulid::new().to_string();

    let source_path = h
        .url
        .clone()
        .unwrap_or_else(|| format!("readwise://highlight/{}", h.id));

    let modified_at = h
        .highlighted_at
        .clone()
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let tags: Vec<String> = h.tags.iter().map(|t| t.name.clone()).collect();

    let mut metadata = serde_json::json!({
        "readwise_id": h.id,
    });
    if let Some(ref title) = h.title {
        metadata["title"] = serde_json::json!(title);
    }
    if let Some(ref author) = h.author {
        metadata["author"] = serde_json::json!(author);
    }
    if let Some(book_id) = h.book_id {
        metadata["book_id"] = serde_json::json!(book_id);
    }

    Fragment {
        id,
        content: h.text.clone(),
        source_type: SourceType::Readwise,
        source_path,
        chunk_index: 0,
        heading_path: Vec::new(),
        tags,
        token_count,
        content_hash: hash,
        modified_at,
        cluster_id: None,
        disposition: fragment::Disposition::Inbox,
        metadata,
    }
}

// ---------------------------------------------------------------------------
// Core API fetch logic
// ---------------------------------------------------------------------------

/// Fetches all highlights from the Readwise API, handling pagination.
///
/// If `since` is provided (ISO 8601), only highlights after that time are
/// returned (incremental sync).
async fn fetch_all_highlights(
    api_key: &str,
    since: Option<&str>,
) -> Result<Vec<ReadwiseHighlight>, SourceError> {
    let client = reqwest::Client::new();
    let mut all_highlights = Vec::new();

    let mut url = format!("{}/highlights/", READWISE_API_BASE);

    // Add updated_after query parameter for incremental sync.
    if let Some(ts) = since {
        url = format!("{}?updated__gt={}", url, ts);
    }

    let mut next_url: Option<String> = Some(url);

    while let Some(current_url) = next_url.take() {
        tracing::debug!("Fetching Readwise highlights: {}", current_url);

        let resp = client
            .get(&current_url)
            .header("Authorization", format!("Token {}", api_key))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SourceError::Http(format!(
                "Readwise API returned {}: {}",
                status, body
            )));
        }

        let page: HighlightsResponse = resp.json().await?;

        tracing::debug!(
            "Fetched page with {} highlights (total count: {})",
            page.results.len(),
            page.count
        );

        all_highlights.extend(page.results);
        next_url = page.next;
    }

    Ok(all_highlights)
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Fetches highlights from the Readwise API (incremental: only new highlights
/// since last sync), converts them to fragments, and returns import statistics.
///
/// Actual embedding and Chroma storage happen downstream in the pipeline.
#[tauri::command]
pub async fn source_readwise_import() -> Result<ReadwiseImportResult, SourceError> {
    let api_key = get_api_key()?;
    let last_sync = get_last_sync_timestamp();

    let highlights = fetch_all_highlights(&api_key, last_sync.as_deref()).await?;
    let total_fetched = highlights.len();

    tracing::info!(
        "Readwise: fetched {} highlights (since: {:?})",
        total_fetched,
        last_sync
    );

    let fragments: Vec<Fragment> = highlights
        .iter()
        .filter(|h| !h.text.trim().is_empty())
        .map(highlight_to_fragment)
        .collect();

    let imported = fragments.len();

    // Embed and store fragments in Chroma.
    if !fragments.is_empty() {
        let grpc = get_grpc_client()?;
        let client = get_client();
        let coll_id = get_collection_id(COLLECTION_READWISE).await?;

        let texts: Vec<String> = fragments.iter().map(|f| f.content.clone()).collect();
        let embeddings = grpc.embed_batch(texts).await?;

        let ids: Vec<String> = fragments.iter().map(|f| f.id.clone()).collect();
        let documents: Vec<String> = fragments.iter().map(|f| f.content.clone()).collect();
        let metadatas: Vec<serde_json::Value> = fragments
            .iter()
            .map(fragment::fragment_to_chroma_metadata)
            .collect();

        client
            .add(&coll_id, ids, Some(embeddings), Some(documents), Some(metadatas))
            .await?;

        tracing::info!("Stored {} Readwise fragments in Chroma", fragments.len());
    }

    // Update the sync timestamp to now.
    let now = chrono::Utc::now().to_rfc3339();
    if let Err(e) = save_last_sync_timestamp(&now) {
        tracing::warn!("Failed to save Readwise sync timestamp: {}", e);
    }

    Ok(ReadwiseImportResult {
        imported,
        total_fetched,
    })
}

/// Saves the Readwise API key to the application config.
#[tauri::command]
pub async fn source_readwise_configure(api_key: String) -> Result<(), SourceError> {
    let mut config = crate::config::load_config()?;
    config.readwise_api_key = Some(api_key);
    crate::config::save_config(&config)?;
    tracing::info!("Readwise API key saved to config");
    Ok(())
}

/// Tests whether the configured Readwise API key is valid by making a
/// lightweight request to the API.
#[tauri::command]
pub async fn source_readwise_check_connection() -> Result<bool, SourceError> {
    let api_key = get_api_key()?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/auth/", READWISE_API_BASE))
        .header("Authorization", format!("Token {}", api_key))
        .send()
        .await?;

    Ok(resp.status().is_success())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_highlight() -> ReadwiseHighlight {
        ReadwiseHighlight {
            id: 42,
            text: "The key insight is that small models can learn structured reasoning."
                .to_string(),
            title: Some("Deep Learning Insights".to_string()),
            author: Some("Author Name".to_string()),
            url: Some("https://example.com/article".to_string()),
            highlighted_at: Some("2025-06-15T12:00:00Z".to_string()),
            book_id: Some(100),
            tags: vec![
                ReadwiseTag {
                    name: "ml".to_string(),
                },
                ReadwiseTag {
                    name: "reasoning".to_string(),
                },
            ],
        }
    }

    #[test]
    fn test_highlight_to_fragment() {
        let h = sample_highlight();
        let f = highlight_to_fragment(&h);

        assert_eq!(f.source_type, SourceType::Readwise);
        assert_eq!(f.source_path, "https://example.com/article");
        assert_eq!(f.chunk_index, 0);
        assert_eq!(f.tags, vec!["ml", "reasoning"]);
        assert_eq!(f.metadata["readwise_id"], 42);
        assert_eq!(f.metadata["title"], "Deep Learning Insights");
        assert_eq!(f.metadata["author"], "Author Name");
        assert_eq!(f.metadata["book_id"], 100);
        assert!(!f.content_hash.is_empty());
    }

    #[test]
    fn test_highlight_to_fragment_minimal() {
        let h = ReadwiseHighlight {
            id: 1,
            text: "Just a highlight.".to_string(),
            title: None,
            author: None,
            url: None,
            highlighted_at: None,
            book_id: None,
            tags: vec![],
        };
        let f = highlight_to_fragment(&h);

        assert_eq!(f.source_path, "readwise://highlight/1");
        assert!(f.tags.is_empty());
        assert!(f.metadata.get("title").is_none());
    }

    #[test]
    fn test_content_hash_deterministic() {
        let hash1 = fragment::content_hash("hello world");
        let hash2 = fragment::content_hash("hello world");
        assert_eq!(hash1, hash2);
        assert_ne!(fragment::content_hash("hello world"), fragment::content_hash("different text"));
    }
}
