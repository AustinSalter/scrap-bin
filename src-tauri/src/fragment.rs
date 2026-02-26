use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A unified content fragment that represents a single chunk of ingested
/// content, regardless of the original source (vault note, tweet, highlight,
/// podcast transcript, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fragment {
    /// Universally unique identifier (ULID).
    pub id: String,
    /// The text content of this fragment.
    pub content: String,
    /// Which ingestion source produced this fragment.
    pub source_type: SourceType,
    /// File path (for local sources) or URL (for remote sources).
    pub source_path: String,
    /// Zero-based index of this chunk within the source document.
    pub chunk_index: usize,
    /// Breadcrumb trail of Markdown headings enclosing this chunk
    /// (e.g. `["Strategy", "Competitive Analysis"]`).
    pub heading_path: Vec<String>,
    /// Tags extracted from the source (frontmatter tags, hashtags, etc.).
    pub tags: Vec<String>,
    /// Estimated token count for this fragment's content.
    pub token_count: usize,
    /// SHA-256 hex digest of `content` (for deduplication).
    pub content_hash: String,
    /// ISO 8601 timestamp of the source's last modification.
    pub modified_at: String,
    /// Cluster assignment from HDBSCAN (None if not yet clustered, or noise).
    pub cluster_id: Option<i32>,
    /// Source-specific extra data that doesn't fit the common fields.
    pub metadata: serde_json::Value,
}

/// Identifies the ingestion source that produced a fragment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Vault,
    Twitter,
    Readwise,
    Podcast,
}

impl SourceType {
    /// Returns the Chroma collection name for this source type.
    pub fn collection_name(&self) -> &'static str {
        match self {
            SourceType::Vault => "vault",
            SourceType::Twitter => "twitter",
            SourceType::Readwise => "readwise",
            SourceType::Podcast => "podcasts",
        }
    }
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            SourceType::Vault => "vault",
            SourceType::Twitter => "twitter",
            SourceType::Readwise => "readwise",
            SourceType::Podcast => "podcast",
        };
        write!(f, "{}", label)
    }
}

// ---------------------------------------------------------------------------
// Shared helpers (used by all source modules)
// ---------------------------------------------------------------------------

/// Computes the SHA-256 hex digest of a string.
pub fn content_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Estimates the token count from character length (~4 chars/token).
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() as f64 / 4.0).ceil() as usize
}

// ---------------------------------------------------------------------------
// Chroma metadata conversion
// ---------------------------------------------------------------------------

/// Converts a `Fragment` into a flat JSON object suitable for Chroma metadata.
///
/// Chroma metadata values must be scalars (string, int, float, bool), so
/// arrays are joined into comma-separated strings.
pub fn fragment_to_chroma_metadata(f: &Fragment) -> serde_json::Value {
    let mut meta = serde_json::json!({
        "id": f.id,
        "source_type": f.source_type.collection_name(),
        "source_path": f.source_path,
        "chunk_index": f.chunk_index,
        "heading_path": f.heading_path.join(" > "),
        "tags": f.tags.join(","),
        "token_count": f.token_count,
        "content_hash": f.content_hash,
        "modified_at": f.modified_at,
        "cluster_id": f.cluster_id.unwrap_or(-1),
    });

    // Merge source-specific metadata. Chroma only supports scalar values,
    // so skip nested objects and arrays.
    if let serde_json::Value::Object(extras) = &f.metadata {
        if let serde_json::Value::Object(ref mut map) = meta {
            for (k, v) in extras {
                if v.is_string() || v.is_number() || v.is_boolean() {
                    map.insert(k.clone(), v.clone());
                }
            }
        }
    }

    meta
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_fragment() -> Fragment {
        Fragment {
            id: "01HXYZ".to_string(),
            content: "Some content".to_string(),
            source_type: SourceType::Vault,
            source_path: "notes/test.md".to_string(),
            chunk_index: 0,
            heading_path: vec!["Strategy".to_string(), "Overview".to_string()],
            tags: vec!["rust".to_string(), "tauri".to_string()],
            token_count: 42,
            content_hash: "abcdef1234567890".to_string(),
            modified_at: "2025-01-15T10:30:00Z".to_string(),
            cluster_id: Some(3),
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn test_source_type_collection_name() {
        assert_eq!(SourceType::Vault.collection_name(), "vault");
        assert_eq!(SourceType::Twitter.collection_name(), "twitter");
        assert_eq!(SourceType::Readwise.collection_name(), "readwise");
        assert_eq!(SourceType::Podcast.collection_name(), "podcasts");
    }

    #[test]
    fn test_source_type_display() {
        assert_eq!(format!("{}", SourceType::Vault), "vault");
        assert_eq!(format!("{}", SourceType::Podcast), "podcast");
    }

    #[test]
    fn test_fragment_to_chroma_metadata() {
        let frag = sample_fragment();
        let meta = fragment_to_chroma_metadata(&frag);

        assert_eq!(meta["source_type"], "vault");
        assert_eq!(meta["source_path"], "notes/test.md");
        assert_eq!(meta["chunk_index"], 0);
        assert_eq!(meta["heading_path"], "Strategy > Overview");
        assert_eq!(meta["tags"], "rust,tauri");
        assert_eq!(meta["token_count"], 42);
        assert_eq!(meta["cluster_id"], 3);
    }

    #[test]
    fn test_fragment_to_chroma_metadata_no_cluster() {
        let mut frag = sample_fragment();
        frag.cluster_id = None;
        let meta = fragment_to_chroma_metadata(&frag);
        assert_eq!(meta["cluster_id"], -1);
    }

    #[test]
    fn test_fragment_to_chroma_metadata_merges_extras() {
        let mut frag = sample_fragment();
        frag.metadata = serde_json::json!({
            "tweet_id": "12345",
            "readwise_id": 42,
            "is_reply": true,
            "nested_obj": {"should": "be skipped"},
            "array_val": [1, 2, 3],
        });
        let meta = fragment_to_chroma_metadata(&frag);

        // Scalars are merged.
        assert_eq!(meta["tweet_id"], "12345");
        assert_eq!(meta["readwise_id"], 42);
        assert_eq!(meta["is_reply"], true);
        // Non-scalars are skipped.
        assert!(meta.get("nested_obj").is_none());
        assert!(meta.get("array_val").is_none());
        // Base fields still present.
        assert_eq!(meta["source_type"], "vault");
    }

    #[test]
    fn test_source_type_serde_roundtrip() {
        let original = SourceType::Twitter;
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(json, "\"twitter\"");
        let deserialized: SourceType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, original);
    }
}
