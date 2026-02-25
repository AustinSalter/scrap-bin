use crate::chroma::client::{get_client, ChromaError};
use crate::chroma::collections::{get_collection_id, COLLECTION_TWITTER};
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
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
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
// Helpers
// ---------------------------------------------------------------------------

// Use shared helpers from fragment and chunker modules.
use crate::chunker;

/// Converts a single Twitter bookmark into one or more `Fragment`s.
fn bookmark_to_fragments(bookmark: &TwitterBookmark) -> Vec<Fragment> {
    // Prefer long-form note_tweet text over the truncated `text` field.
    let full_text = bookmark
        .note_tweet
        .as_ref()
        .and_then(|nt| nt.text.as_deref())
        .unwrap_or(&bookmark.text);

    let source_path_str = format!("twitter://bookmark/{}", bookmark.id);
    let chunked = chunker::chunk_plain_text(full_text, &source_path_str);
    let chunks: Vec<String> = chunked.iter().map(|c| c.content.clone()).collect();
    let modified_at = bookmark
        .created_at
        .clone()
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    chunks
        .into_iter()
        .enumerate()
        .map(|(idx, chunk_text)| {
            let hash = fragment::content_hash(&chunk_text);
            let token_count = fragment::estimate_tokens(&chunk_text);
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
                source_path: source_path_str.clone(),
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
    // Query Chroma for existing tweet IDs to deduplicate.
    let client = get_client();
    let coll_id = get_collection_id(COLLECTION_TWITTER).await?;
    let existing_result = client.get(&coll_id, None, None, Some(vec!["metadatas".to_string()])).await;
    let mut existing_ids = std::collections::HashSet::new();
    if let Ok(result) = existing_result {
        if let Some(metas) = &result.metadatas {
            for meta in metas.iter().flatten() {
                if let Some(tid) = meta.get("tweet_id").and_then(|v| v.as_str()) {
                    existing_ids.insert(tid.to_string());
                }
            }
        }
    }

    let (fragments, result) = tokio::task::spawn_blocking(move || {
        import_bookmarks(&path, &existing_ids)
    })
    .await
    .map_err(|e| SourceError::InvalidData(format!("Task join error: {e}")))?
    ?;

    // Embed and store fragments in Chroma.
    if !fragments.is_empty() {
        let grpc = get_grpc_client()?;
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

        tracing::info!("Stored {} Twitter fragments in Chroma", fragments.len());
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
