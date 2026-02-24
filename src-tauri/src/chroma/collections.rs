use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::client::{get_client, ChromaError, CollectionInfo};

// ---------------------------------------------------------------------------
// Collection name constants
// ---------------------------------------------------------------------------

pub const COLLECTION_VAULT: &str = "vault";
pub const COLLECTION_TWITTER: &str = "twitter";
pub const COLLECTION_READWISE: &str = "readwise";
pub const COLLECTION_PODCASTS: &str = "podcasts";
pub const COLLECTION_CLUSTERS: &str = "clusters";
pub const COLLECTION_THREADS: &str = "threads";

pub const ALL_COLLECTIONS: &[&str] = &[
    COLLECTION_VAULT,
    COLLECTION_TWITTER,
    COLLECTION_READWISE,
    COLLECTION_PODCASTS,
    COLLECTION_CLUSTERS,
    COLLECTION_THREADS,
];

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionStats {
    pub name: String,
    pub count: usize,
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// Creates all predefined collections using `get_or_create`, returning a map
/// from collection name to its `CollectionInfo`.
pub async fn ensure_all_collections() -> Result<HashMap<String, CollectionInfo>, ChromaError> {
    let client = get_client();
    let mut result = HashMap::new();

    for name in ALL_COLLECTIONS {
        let info = client.create_collection(name).await?;
        result.insert(name.to_string(), info);
    }

    Ok(result)
}

/// Resolves a collection name to its Chroma-assigned UUID.
pub async fn get_collection_id(name: &str) -> Result<String, ChromaError> {
    let client = get_client();
    let info = client.get_collection(name).await?;
    Ok(info.id)
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Lists all collections visible to the Chroma client.
#[tauri::command]
pub async fn chroma_list_collections() -> Result<Vec<CollectionInfo>, ChromaError> {
    let client = get_client();
    client.list_collections().await
}

/// Returns the name and document count for every predefined collection.
#[tauri::command]
pub async fn chroma_get_collection_stats() -> Result<Vec<CollectionStats>, ChromaError> {
    let client = get_client();
    let mut stats = Vec::with_capacity(ALL_COLLECTIONS.len());

    for name in ALL_COLLECTIONS {
        // Attempt to get the collection; if it does not exist yet, report
        // count as 0 rather than erroring out.
        match client.get_collection(name).await {
            Ok(info) => {
                let count = client.count(&info.id).await.unwrap_or(0);
                stats.push(CollectionStats {
                    name: name.to_string(),
                    count,
                });
            }
            Err(ChromaError::CollectionNotFound(_)) => {
                stats.push(CollectionStats {
                    name: name.to_string(),
                    count: 0,
                });
            }
            Err(e) => return Err(e),
        }
    }

    Ok(stats)
}
