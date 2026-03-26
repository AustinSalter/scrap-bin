use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::chroma::client::{get_client, ChromaError};
use crate::chroma::collections::{
    get_collection_id, CONTENT_COLLECTIONS,
};
use crate::fragment::{chroma_to_fragment, Fragment, SourceType};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

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
    #[error("Invalid collection name: {0}")]
    InvalidCollectionName(String),
    #[error("Fragment not found: {0}")]
    FragmentNotFound(String),
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
        _ => CONTENT_COLLECTIONS.iter().map(|s| s.to_string()).collect(),
    }
}

/// Query a single collection and return a vec of SearchResult.
///
/// Uses Chroma's `query_texts` endpoint, which handles embedding internally
/// — no Python sidecar call is needed in the retrieval path.
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
/// — no Python sidecar call is needed in the retrieval path.
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
        return Err(SearchError::InvalidCollectionName(collection));
    }

    let n_results = params.n_results.unwrap_or(DEFAULT_N_RESULTS);

    query_single_collection(&collection, query, n_results, params.cluster_id).await
}

// ---------------------------------------------------------------------------
// Fragment query types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentFilter {
    pub source_type: Option<String>,
    pub disposition: Option<String>,
    /// Collection name override (takes priority over source_type).
    pub scope: Option<String>,
    /// Page number (0-indexed). Default: 0.
    pub page: Option<usize>,
    /// Results per page. Default: 50.
    pub page_size: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentPage {
    pub fragments: Vec<Fragment>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispositionCounts {
    pub signal: usize,
    pub inbox: usize,
    pub ignored: usize,
}

// ---------------------------------------------------------------------------
// Fragment query helpers
// ---------------------------------------------------------------------------

/// Determine which collections to query based on the filter.
fn resolve_fragment_collections(filter: &FragmentFilter) -> Vec<&'static str> {
    if let Some(ref scope) = filter.scope {
        // Check if it's a valid content collection name.
        for &coll in CONTENT_COLLECTIONS {
            if coll == scope.as_str() {
                return vec![coll];
            }
        }
        return Vec::new();
    }

    if let Some(ref st) = filter.source_type {
        if let Some(source) = SourceType::from_collection_name(st) {
            return vec![source.collection_name()];
        }
        // Also try matching on the source_type Display name (e.g. "podcast" → "podcasts").
        let all_types = [
            SourceType::Vault,
            SourceType::Twitter,
            SourceType::Readwise,
            SourceType::Podcast,
            SourceType::Rss,
            SourceType::AppleNotes,
        ];
        for source in &all_types {
            if source.to_string() == st.as_str() {
                return vec![source.collection_name()];
            }
        }
        return Vec::new();
    }

    CONTENT_COLLECTIONS.to_vec()
}

// ---------------------------------------------------------------------------
// Fragment query Tauri commands
// ---------------------------------------------------------------------------

/// List fragments with optional filters, sorted by modified_at descending.
// TODO: Pagination is done in-memory after fetching all matching fragments.
// At scale (>10K fragments), push sorting/pagination to Chroma or add a
// local index. Cross-collection sorted pagination prevents using Chroma's
// native limit/offset directly.
#[tauri::command]
pub async fn list_fragments(
    filter: FragmentFilter,
) -> Result<FragmentPage, SearchError> {
    let client = get_client();
    let collections = resolve_fragment_collections(&filter);
    let page = filter.page.unwrap_or(0);
    let page_size = filter.page_size.unwrap_or(50);

    let where_filter = filter
        .disposition
        .as_ref()
        .map(|d| serde_json::json!({ "disposition": d }));

    let mut all_fragments: Vec<Fragment> = Vec::new();

    for coll_name in &collections {
        let coll_id = match get_collection_id(coll_name).await {
            Ok(id) => id,
            Err(ChromaError::CollectionNotFound(_)) => continue,
            Err(e) => return Err(SearchError::Chroma(e)),
        };

        let result = client
            .get(
                &coll_id,
                None,
                where_filter.clone(),
                Some(vec![
                    "documents".to_string(),
                    "metadatas".to_string(),
                ]),
                None,
                None,
            )
            .await;

        let result = match result {
            Ok(r) => r,
            Err(_) => continue,
        };

        for (i, id) in result.ids.iter().enumerate() {
            let doc = result
                .documents
                .as_ref()
                .and_then(|docs| docs.get(i).cloned())
                .flatten()
                .unwrap_or_default();
            let meta = result
                .metadatas
                .as_ref()
                .and_then(|metas| metas.get(i).cloned())
                .flatten()
                .unwrap_or(serde_json::json!({}));

            all_fragments.push(chroma_to_fragment(id.clone(), doc, &meta));
        }
    }

    // Sort by modified_at descending.
    all_fragments.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));

    let total = all_fragments.len();
    let start = page * page_size;
    let fragments: Vec<Fragment> = all_fragments
        .into_iter()
        .skip(start)
        .take(page_size)
        .collect();

    Ok(FragmentPage {
        fragments,
        total,
        page,
        page_size,
    })
}

/// Get a single fragment by ID, searching across all content collections.
#[tauri::command]
pub async fn get_fragment(id: String) -> Result<Fragment, SearchError> {
    let client = get_client();

    for coll_name in CONTENT_COLLECTIONS {
        let coll_id = match get_collection_id(coll_name).await {
            Ok(id) => id,
            Err(ChromaError::CollectionNotFound(_)) => continue,
            Err(e) => return Err(SearchError::Chroma(e)),
        };

        let result = client
            .get(
                &coll_id,
                Some(vec![id.clone()]),
                None,
                Some(vec![
                    "documents".to_string(),
                    "metadatas".to_string(),
                ]),
                None,
                None,
            )
            .await;

        if let Ok(result) = result {
            if !result.ids.is_empty() {
                let doc = result
                    .documents
                    .as_ref()
                    .and_then(|docs| docs.first().cloned())
                    .flatten()
                    .unwrap_or_default();
                let meta = result
                    .metadatas
                    .as_ref()
                    .and_then(|metas| metas.first().cloned())
                    .flatten()
                    .unwrap_or(serde_json::json!({}));

                return Ok(chroma_to_fragment(id, doc, &meta));
            }
        }
    }

    Err(SearchError::FragmentNotFound(id))
}

/// Count fragments by disposition across all content collections.
/// Fetches metadata once per collection (N calls) and counts locally.
#[tauri::command]
pub async fn get_disposition_counts() -> Result<DispositionCounts, SearchError> {
    let client = get_client();
    let mut signal: usize = 0;
    let mut inbox: usize = 0;
    let mut ignored: usize = 0;

    for coll_name in CONTENT_COLLECTIONS {
        let coll_id = match get_collection_id(coll_name).await {
            Ok(id) => id,
            Err(ChromaError::CollectionNotFound(_)) => continue,
            Err(e) => return Err(SearchError::Chroma(e)),
        };

        let result = client
            .get(
                &coll_id,
                None,
                None,
                Some(vec!["metadatas".to_string()]),
                None,
                None,
            )
            .await;

        if let Ok(result) = result {
            if let Some(metadatas) = &result.metadatas {
                for meta in metadatas {
                    let disp = meta
                        .as_ref()
                        .and_then(|m| m.get("disposition"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("inbox");
                    match disp {
                        "signal" => signal += 1,
                        "inbox" => inbox += 1,
                        "ignored" => ignored += 1,
                        _ => inbox += 1,
                    }
                }
            }
        }
    }

    Ok(DispositionCounts {
        signal,
        inbox,
        ignored,
    })
}

/// Convenience command: returns inbox fragments, optionally filtered by source.
#[tauri::command]
pub async fn get_inbox(
    source_filter: Option<String>,
    page: Option<usize>,
    page_size: Option<usize>,
) -> Result<FragmentPage, SearchError> {
    let filter = FragmentFilter {
        source_type: source_filter,
        disposition: Some("inbox".to_string()),
        scope: None,
        page,
        page_size,
    };
    list_fragments(filter).await
}
