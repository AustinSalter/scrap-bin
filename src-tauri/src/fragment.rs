use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Triage state for a fragment: signal (keep), inbox (unprocessed), or ignored.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Signal,
    Inbox,
    Ignored,
}

impl Default for Disposition {
    fn default() -> Self {
        Disposition::Inbox
    }
}

impl std::fmt::Display for Disposition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Disposition::Signal => "signal",
            Disposition::Inbox => "inbox",
            Disposition::Ignored => "ignored",
        };
        write!(f, "{}", label)
    }
}

impl std::str::FromStr for Disposition {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "signal" => Ok(Disposition::Signal),
            "inbox" => Ok(Disposition::Inbox),
            "ignored" => Ok(Disposition::Ignored),
            _ => Err(format!("unknown disposition: {}", s)),
        }
    }
}

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
    /// Triage disposition: signal, inbox, or ignored.
    pub disposition: Disposition,
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
    Rss,
    AppleNotes,
}

impl SourceType {
    /// Returns the Chroma collection name for this source type.
    pub fn collection_name(&self) -> &'static str {
        match self {
            SourceType::Vault => "vault",
            SourceType::Twitter => "twitter",
            SourceType::Readwise => "readwise",
            SourceType::Podcast => "podcasts",
            SourceType::Rss => "rss",
            SourceType::AppleNotes => "apple_notes",
        }
    }

    /// Reverse lookup: Chroma collection name → SourceType.
    pub fn from_collection_name(name: &str) -> Option<SourceType> {
        match name {
            "vault" => Some(SourceType::Vault),
            "twitter" => Some(SourceType::Twitter),
            "readwise" => Some(SourceType::Readwise),
            "podcasts" => Some(SourceType::Podcast),
            "rss" => Some(SourceType::Rss),
            "apple_notes" => Some(SourceType::AppleNotes),
            _ => None,
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
            SourceType::Rss => "rss",
            SourceType::AppleNotes => "apple_notes",
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
        "disposition": f.disposition.to_string(),
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

/// Known metadata keys produced by `fragment_to_chroma_metadata()`.
/// Used by `chroma_to_fragment()` to separate known fields from extras.
const KNOWN_META_KEYS: &[&str] = &[
    "id",
    "source_type",
    "source_path",
    "chunk_index",
    "heading_path",
    "tags",
    "token_count",
    "content_hash",
    "modified_at",
    "cluster_id",
    "disposition",
];

/// Reconstructs a `Fragment` from a Chroma ID, document, and metadata JSON.
/// This is the inverse of `fragment_to_chroma_metadata()`.
pub fn chroma_to_fragment(
    id: String,
    content: String,
    metadata: &serde_json::Value,
) -> Fragment {
    let source_type = metadata
        .get("source_type")
        .and_then(|v| v.as_str())
        .and_then(SourceType::from_collection_name)
        .unwrap_or(SourceType::Vault);

    let source_path = metadata
        .get("source_path")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let chunk_index = metadata
        .get("chunk_index")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    let heading_path: Vec<String> = metadata
        .get("heading_path")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.split(" > ").map(|p| p.to_string()).collect())
        .unwrap_or_default();

    let tags: Vec<String> = metadata
        .get("tags")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.split(',').map(|t| t.to_string()).collect())
        .unwrap_or_default();

    let token_count = metadata
        .get("token_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    let content_hash_val = metadata
        .get("content_hash")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let modified_at = metadata
        .get("modified_at")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let cluster_id = metadata
        .get("cluster_id")
        .and_then(|v| v.as_i64())
        .map(|v| if v == -1 { None } else { Some(v as i32) })
        .unwrap_or(None);

    let disposition = metadata
        .get("disposition")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<Disposition>().ok())
        .unwrap_or_default();

    // Collect remaining metadata keys into the catch-all `metadata` field.
    let extras = if let serde_json::Value::Object(map) = metadata {
        let mut extra_map = serde_json::Map::new();
        for (k, v) in map {
            if !KNOWN_META_KEYS.contains(&k.as_str()) {
                extra_map.insert(k.clone(), v.clone());
            }
        }
        serde_json::Value::Object(extra_map)
    } else {
        serde_json::json!({})
    };

    Fragment {
        id,
        content,
        source_type,
        source_path,
        chunk_index,
        heading_path,
        tags,
        token_count,
        content_hash: content_hash_val,
        modified_at,
        cluster_id,
        disposition,
        metadata: extras,
    }
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
            disposition: Disposition::Inbox,
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn test_source_type_collection_name() {
        assert_eq!(SourceType::Vault.collection_name(), "vault");
        assert_eq!(SourceType::Twitter.collection_name(), "twitter");
        assert_eq!(SourceType::Readwise.collection_name(), "readwise");
        assert_eq!(SourceType::Podcast.collection_name(), "podcasts");
        assert_eq!(SourceType::Rss.collection_name(), "rss");
        assert_eq!(SourceType::AppleNotes.collection_name(), "apple_notes");
    }

    #[test]
    fn test_source_type_display() {
        assert_eq!(format!("{}", SourceType::Vault), "vault");
        assert_eq!(format!("{}", SourceType::Podcast), "podcast");
        assert_eq!(format!("{}", SourceType::Rss), "rss");
        assert_eq!(format!("{}", SourceType::AppleNotes), "apple_notes");
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
        assert_eq!(meta["disposition"], "inbox");
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

        let rss = SourceType::Rss;
        let json = serde_json::to_string(&rss).unwrap();
        assert_eq!(json, "\"rss\"");
        let deserialized: SourceType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, rss);
    }

    #[test]
    fn test_disposition_display() {
        assert_eq!(format!("{}", Disposition::Signal), "signal");
        assert_eq!(format!("{}", Disposition::Inbox), "inbox");
        assert_eq!(format!("{}", Disposition::Ignored), "ignored");
    }

    #[test]
    fn test_disposition_serde_roundtrip() {
        let original = Disposition::Signal;
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(json, "\"signal\"");
        let deserialized: Disposition = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, original);
    }

    #[test]
    fn test_disposition_default() {
        assert_eq!(Disposition::default(), Disposition::Inbox);
    }

    #[test]
    fn test_disposition_from_str() {
        assert_eq!("signal".parse::<Disposition>().unwrap(), Disposition::Signal);
        assert_eq!("inbox".parse::<Disposition>().unwrap(), Disposition::Inbox);
        assert_eq!("ignored".parse::<Disposition>().unwrap(), Disposition::Ignored);
        assert!("unknown".parse::<Disposition>().is_err());
    }

    #[test]
    fn test_source_type_from_collection_name() {
        assert_eq!(SourceType::from_collection_name("vault"), Some(SourceType::Vault));
        assert_eq!(SourceType::from_collection_name("twitter"), Some(SourceType::Twitter));
        assert_eq!(SourceType::from_collection_name("readwise"), Some(SourceType::Readwise));
        assert_eq!(SourceType::from_collection_name("podcasts"), Some(SourceType::Podcast));
        assert_eq!(SourceType::from_collection_name("rss"), Some(SourceType::Rss));
        assert_eq!(SourceType::from_collection_name("apple_notes"), Some(SourceType::AppleNotes));
        assert_eq!(SourceType::from_collection_name("unknown"), None);
    }

    #[test]
    fn test_chroma_to_fragment_roundtrip() {
        let original = sample_fragment();
        let meta = fragment_to_chroma_metadata(&original);
        let reconstructed = chroma_to_fragment(
            original.id.clone(),
            original.content.clone(),
            &meta,
        );

        assert_eq!(reconstructed.id, original.id);
        assert_eq!(reconstructed.content, original.content);
        assert_eq!(reconstructed.source_type, original.source_type);
        assert_eq!(reconstructed.source_path, original.source_path);
        assert_eq!(reconstructed.chunk_index, original.chunk_index);
        assert_eq!(reconstructed.heading_path, original.heading_path);
        assert_eq!(reconstructed.tags, original.tags);
        assert_eq!(reconstructed.token_count, original.token_count);
        assert_eq!(reconstructed.content_hash, original.content_hash);
        assert_eq!(reconstructed.modified_at, original.modified_at);
        assert_eq!(reconstructed.cluster_id, original.cluster_id);
        assert_eq!(reconstructed.disposition, original.disposition);
    }

    #[test]
    fn test_chroma_to_fragment_no_cluster() {
        let mut frag = sample_fragment();
        frag.cluster_id = None;
        let meta = fragment_to_chroma_metadata(&frag);
        let reconstructed = chroma_to_fragment(frag.id.clone(), frag.content.clone(), &meta);
        assert_eq!(reconstructed.cluster_id, None);
    }

    #[test]
    fn test_chroma_to_fragment_extras() {
        let mut frag = sample_fragment();
        frag.metadata = serde_json::json!({ "tweet_id": "12345" });
        let meta = fragment_to_chroma_metadata(&frag);
        let reconstructed = chroma_to_fragment(frag.id.clone(), frag.content.clone(), &meta);
        assert_eq!(reconstructed.metadata.get("tweet_id").and_then(|v| v.as_str()), Some("12345"));
    }
}
