use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::chroma::client::{get_client, ChromaError};
use crate::chroma::collections::{
    get_collection_id, COLLECTION_PODCASTS, COLLECTION_READWISE, COLLECTION_TWITTER,
    COLLECTION_VAULT,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Content collections that are searched by default.
const DEFAULT_COLLECTIONS: &[&str] = &[
    COLLECTION_VAULT,
    COLLECTION_TWITTER,
    COLLECTION_READWISE,
    COLLECTION_PODCASTS,
];

const DEFAULT_N_RESULTS: usize = 10;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum SearchError {
    #[error("Chroma error: {0}")]
    Chroma(#[from] ChromaError),
    #[error("Empty query")]
    EmptyQuery,
    #[error("No collections to search")]
    NoCollections,
}

impl Serialize for SearchError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchParams {
    pub query: String,
    /// Maximum number of results to return. Default: 10.
    pub n_results: Option<usize>,
    /// Filter by specific collections. `None` means all content collections.
    pub source_types: Option<Vec<String>>,
    /// Filter results by cluster assignment.
    pub cluster_id: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub content: String,
    pub source_type: String,
    pub source_path: String,
    pub distance: f64,
    pub metadata: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Determine which collections to query based on the params.
fn resolve_search_collections(source_types: &Option<Vec<String>>) -> Vec<String> {
    match source_types {
        Some(types) if !types.is_empty() => types.clone(),
        _ => DEFAULT_COLLECTIONS.iter().map(|s| s.to_string()).collect(),
    }
}

/// Query a single collection and return a vec of SearchResult.
async fn query_single_collection(
    collection_name: &str,
    query: &str,
    n_results: usize,
    cluster_id: Option<i32>,
) -> Result<Vec<SearchResult>, SearchError> {
    let client = get_client();
    let coll_id = match get_collection_id(collection_name).await {
        Ok(id) => id,
        Err(ChromaError::CollectionNotFound(_)) => return Ok(Vec::new()),
        Err(e) => return Err(SearchError::Chroma(e)),
    };

    let where_filter = cluster_id.map(|cid| serde_json::json!({ "cluster_id": cid }));

    let result = client
        .query(
            &coll_id,
            Some(vec![query.to_string()]),
            None,
            n_results,
            where_filter,
        )
        .await?;

    // Chroma query returns nested vecs (one per query text). We sent one
    // query, so we take the first element of each outer vec.
    let ids = result.ids.first().cloned().unwrap_or_default();
    let distances = result
        .distances
        .as_ref()
        .and_then(|d| d.first())
        .cloned()
        .unwrap_or_default();
    let documents = result
        .documents
        .as_ref()
        .and_then(|d| d.first())
        .cloned()
        .unwrap_or_default();
    let metadatas = result
        .metadatas
        .as_ref()
        .and_then(|m| m.first())
        .cloned()
        .unwrap_or_default();

    let mut results: Vec<SearchResult> = Vec::with_capacity(ids.len());

    for (i, id) in ids.iter().enumerate() {
        let content = documents
            .get(i)
            .and_then(|d| d.clone())
            .unwrap_or_default();
        let distance = distances.get(i).copied().unwrap_or(f64::MAX);
        let metadata = metadatas
            .get(i)
            .and_then(|m| m.clone())
            .unwrap_or(serde_json::json!({}));
        let source_path = metadata
            .get("source_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        results.push(SearchResult {
            id: id.clone(),
            content,
            source_type: collection_name.to_string(),
            source_path,
            distance,
            metadata,
        });
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Search across all specified collections (or all content collections by
/// default), merge results sorted by distance, and return the top `n_results`.
///
/// Uses Chroma's `query_texts` endpoint, which handles embedding internally
/// -- no Python sidecar call is needed in the retrieval path.
#[tauri::command]
pub async fn search_all(
    params: SearchParams,
) -> Result<Vec<SearchResult>, SearchError> {
    let query = params.query.trim();
    if query.is_empty() {
        return Err(SearchError::EmptyQuery);
    }

    let n_results = params.n_results.unwrap_or(DEFAULT_N_RESULTS);
    let collections = resolve_search_collections(&params.source_types);

    if collections.is_empty() {
        return Err(SearchError::NoCollections);
    }

    // Query each collection for n_results (we will merge and truncate after).
    let mut all_results: Vec<SearchResult> = Vec::new();

    for coll_name in &collections {
        match query_single_collection(coll_name, query, n_results, params.cluster_id).await {
            Ok(results) => all_results.extend(results),
            Err(e) => {
                tracing::warn!("Search in collection '{}' failed: {}", coll_name, e);
                // Continue searching other collections.
            }
        }
    }

    // Sort by distance (ascending = most similar first).
    all_results.sort_by(|a, b| {
        a.distance
            .partial_cmp(&b.distance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Truncate to the requested number of results.
    all_results.truncate(n_results);

    Ok(all_results)
}

/// Validate that a collection name contains only safe characters.
fn is_valid_collection_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Search a single named collection.
#[tauri::command]
pub async fn search_collection(
    collection: String,
    params: SearchParams,
) -> Result<Vec<SearchResult>, SearchError> {
    let query = params.query.trim();
    if query.is_empty() {
        return Err(SearchError::EmptyQuery);
    }

    if !is_valid_collection_name(&collection) {
        return Err(SearchError::NoCollections);
    }

    let n_results = params.n_results.unwrap_or(DEFAULT_N_RESULTS);

    query_single_collection(&collection, query, n_results, params.cluster_id).await
}
