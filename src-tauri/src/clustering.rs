use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::chroma::client::{get_client, ChromaError};
use crate::chroma::collections::{
    get_collection_id, COLLECTION_CLUSTERS, COLLECTION_VAULT, CONTENT_COLLECTIONS,
};
use crate::grpc_client::{get_grpc_client, GrpcError};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum ClusteringError {
    #[error("Chroma error: {0}")]
    Chroma(#[from] ChromaError),
    #[error("gRPC error: {0}")]
    Grpc(#[from] GrpcError),
    #[error("No embeddings found")]
    NoEmbeddings,
    #[error("Cluster not found: {0}")]
    ClusterNotFound(i32),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

impl Serialize for ClusteringError {
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
pub struct ClusterParams {
    pub min_cluster_size: Option<i32>,
    pub min_samples: Option<i32>,
    /// Which content collections to cluster. `None` means all content
    /// collections (vault, twitter, readwise, podcasts).
    pub collections: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterPosition {
    pub label: i32,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterView {
    pub label: i32,
    pub display_label: String,
    pub size: usize,
    pub pinned: bool,
    pub fragment_ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Determine which content collections to cluster based on params.
fn resolve_collections(collections: &Option<Vec<String>>) -> Vec<String> {
    match collections {
        Some(cols) if !cols.is_empty() => cols.clone(),
        _ => CONTENT_COLLECTIONS.iter().map(|s| s.to_string()).collect(),
    }
}

/// Generate a display label from the first fragment's content or heading.
fn auto_label(documents: &[Option<String>]) -> String {
    for doc in documents {
        if let Some(text) = doc {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            // Use first line if it looks like a heading, otherwise first 50 chars.
            let first_line = trimmed.lines().next().unwrap_or(trimmed);
            let label = first_line.trim_start_matches('#').trim();
            if label.len() <= 50 {
                return label.to_string();
            }
            // Truncate at a char boundary to avoid panicking on multi-byte UTF-8.
            let truncated: String = label.chars().take(47).collect();
            return format!("{truncated}...");
        }
    }
    "Unlabeled cluster".to_string()
}

/// Compute the element-wise mean of a set of embedding vectors.
fn compute_centroid(embeddings: &[Vec<f32>]) -> Vec<f32> {
    if embeddings.is_empty() {
        return Vec::new();
    }
    let dim = embeddings[0].len();
    let mut centroid = vec![0.0f32; dim];
    for emb in embeddings {
        for (i, val) in emb.iter().enumerate() {
            if i < dim {
                centroid[i] += val;
            }
        }
    }
    let n = embeddings.len() as f32;
    for val in centroid.iter_mut() {
        *val /= n;
    }
    centroid
}

/// Allocate the next cluster label by scanning existing cluster metadata in
/// the clusters collection and returning max_label + 1.
async fn next_cluster_label() -> Result<i32, ClusteringError> {
    let client = get_client();
    let coll_id = get_collection_id(COLLECTION_CLUSTERS).await?;

    let result = client
        .get(&coll_id, None, None, Some(vec!["metadatas".to_string()]))
        .await?;

    let mut max_label: i32 = -1;
    if let Some(metas) = &result.metadatas {
        for meta in metas {
            if let Some(meta_val) = meta {
                if let Some(label) = meta_val.get("cluster_label").and_then(|v| v.as_i64()) {
                    let label = label as i32;
                    if label > max_label {
                        max_label = label;
                    }
                }
            }
        }
    }

    Ok(max_label + 1)
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Run HDBSCAN clustering: fetch embeddings from Chroma content collections,
/// send to Python sidecar, persist cluster assignments back as metadata, and
/// store cluster summaries in the `clusters` collection.
#[tauri::command]
pub async fn clustering_run(
    params: ClusterParams,
) -> Result<Vec<ClusterView>, ClusteringError> {
    let client = get_client();
    let grpc = get_grpc_client()?;
    let collections = resolve_collections(&params.collections);

    let min_cluster_size = params.min_cluster_size.unwrap_or(5);
    let min_samples = params.min_samples.unwrap_or(3);

    // ---- 1. Gather embeddings from all target content collections ----------
    let mut all_ids: Vec<String> = Vec::new();
    let mut all_embeddings: Vec<Vec<f32>> = Vec::new();
    let mut all_documents: Vec<Option<String>> = Vec::new();
    let mut id_to_collection: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for coll_name in &collections {
        let coll_id = match get_collection_id(coll_name).await {
            Ok(id) => id,
            Err(ChromaError::CollectionNotFound(_)) => continue,
            Err(e) => return Err(ClusteringError::Chroma(e)),
        };

        let result = client
            .get(
                &coll_id,
                None,
                None,
                Some(vec![
                    "embeddings".to_string(),
                    "documents".to_string(),
                ]),
            )
            .await?;

        if let Some(ref embs) = result.embeddings {
            for (i, id) in result.ids.iter().enumerate() {
                all_ids.push(id.clone());
                all_embeddings.push(embs[i].clone());
                id_to_collection.insert(id.clone(), coll_name.clone());

                let doc = result
                    .documents
                    .as_ref()
                    .and_then(|docs| docs.get(i).cloned())
                    .flatten();
                all_documents.push(doc);
            }
        }
    }

    if all_embeddings.is_empty() {
        return Err(ClusteringError::NoEmbeddings);
    }

    // ---- 2. Call Python sidecar for HDBSCAN clustering ---------------------
    let ids_for_cluster = all_ids.clone();
    let cluster_result = grpc
        .cluster(
            all_embeddings,
            ids_for_cluster,
            min_cluster_size,
            min_samples,
        )
        .await?;

    // ---- 3. Persist cluster_id metadata back onto content fragments --------
    // Group fragment IDs by their source collection for batch updates.
    let mut updates_by_collection: std::collections::HashMap<String, (Vec<String>, Vec<serde_json::Value>)> =
        std::collections::HashMap::new();

    for (i, id) in all_ids.iter().enumerate() {
        let label = cluster_result.labels.get(i).copied().unwrap_or(-1);
        if let Some(coll_name) = id_to_collection.get(id) {
            let entry = updates_by_collection
                .entry(coll_name.clone())
                .or_insert_with(|| (Vec::new(), Vec::new()));
            entry.0.push(id.clone());
            entry.1.push(serde_json::json!({ "cluster_id": label }));
        }
    }

    for (coll_name, (ids, metadatas)) in &updates_by_collection {
        let coll_id = get_collection_id(coll_name).await?;
        client
            .update(&coll_id, ids.clone(), None, None, Some(metadatas.clone()))
            .await?;
    }

    // ---- 4. Build ClusterViews and persist to clusters collection ----------
    let clusters_coll_id = get_collection_id(COLLECTION_CLUSTERS).await?;

    // Clear previous cluster entries.
    let existing = client
        .get(&clusters_coll_id, None, None, None)
        .await?;
    if !existing.ids.is_empty() {
        client.delete(&clusters_coll_id, existing.ids).await?;
    }

    let mut views: Vec<ClusterView> = Vec::new();

    for cluster_info in &cluster_result.clusters {
        // Gather documents for this cluster to generate a label.
        let member_docs: Vec<Option<String>> = cluster_info
            .member_ids
            .iter()
            .filter_map(|mid| {
                all_ids
                    .iter()
                    .position(|id| id == mid)
                    .map(|idx| all_documents.get(idx).cloned().flatten())
            })
            .collect();

        let display_label = auto_label(&member_docs);
        let centroid = cluster_info.centroid.clone();

        let view = ClusterView {
            label: cluster_info.label,
            display_label: display_label.clone(),
            size: cluster_info.size as usize,
            pinned: false,
            fragment_ids: cluster_info.member_ids.clone(),
        };

        // Persist to clusters collection: use label as ID, centroid as
        // embedding, and metadata for display_label / pinned / fragment count.
        let doc_id = format!("cluster_{}", cluster_info.label);
        client
            .add(
                &clusters_coll_id,
                vec![doc_id],
                Some(vec![centroid]),
                Some(vec![display_label.clone()]),
                Some(vec![serde_json::json!({
                    "cluster_label": cluster_info.label,
                    "display_label": display_label,
                    "size": cluster_info.size,
                    "pinned": false,
                    "fragment_ids": cluster_info.member_ids.join(","),
                })]),
            )
            .await?;

        views.push(view);
    }

    tracing::info!(
        "Clustering complete: {} clusters, {} noise fragments",
        views.len(),
        cluster_result.noise_count
    );

    Ok(views)
}

/// Return cached cluster views from the `clusters` Chroma collection.
#[tauri::command]
pub async fn clustering_get_all() -> Result<Vec<ClusterView>, ClusteringError> {
    let client = get_client();
    let coll_id = get_collection_id(COLLECTION_CLUSTERS).await?;

    let result = client
        .get(
            &coll_id,
            None,
            None,
            Some(vec!["metadatas".to_string(), "documents".to_string()]),
        )
        .await?;

    let mut views: Vec<ClusterView> = Vec::new();

    if let Some(ref metas) = result.metadatas {
        for (i, meta_opt) in metas.iter().enumerate() {
            if let Some(meta) = meta_opt {
                let label = meta
                    .get("cluster_label")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(-1) as i32;
                let display_label = meta
                    .get("display_label")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unlabeled")
                    .to_string();
                let size = meta
                    .get("size")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                let pinned = meta
                    .get("pinned")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let fragment_ids: Vec<String> = meta
                    .get("fragment_ids")
                    .and_then(|v| v.as_str())
                    .map(|s| {
                        s.split(',')
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                            .collect()
                    })
                    .unwrap_or_default();

                views.push(ClusterView {
                    label,
                    display_label,
                    size,
                    pinned,
                    fragment_ids,
                });
            }
        }
    }

    // Sort by label for consistent ordering.
    views.sort_by_key(|v| v.label);

    Ok(views)
}

/// Get all fragments belonging to a specific cluster by querying content
/// collections where `cluster_id` matches.
#[tauri::command]
pub async fn clustering_get_fragments(
    cluster_id: i32,
) -> Result<Vec<serde_json::Value>, ClusteringError> {
    let client = get_client();
    let where_filter = serde_json::json!({ "cluster_id": cluster_id });
    let mut fragments: Vec<serde_json::Value> = Vec::new();

    for coll_name in CONTENT_COLLECTIONS {
        let coll_id = match get_collection_id(coll_name).await {
            Ok(id) => id,
            Err(ChromaError::CollectionNotFound(_)) => continue,
            Err(e) => return Err(ClusteringError::Chroma(e)),
        };

        let result = client
            .get(
                &coll_id,
                None,
                Some(where_filter.clone()),
                Some(vec![
                    "documents".to_string(),
                    "metadatas".to_string(),
                ]),
            )
            .await?;

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

            fragments.push(serde_json::json!({
                "id": id,
                "content": doc,
                "source_type": coll_name,
                "metadata": meta,
            }));
        }
    }

    Ok(fragments)
}

/// Get all fragments with `cluster_id = -1` (HDBSCAN noise / orphans).
#[tauri::command]
pub async fn clustering_get_orphans() -> Result<Vec<serde_json::Value>, ClusteringError> {
    clustering_get_fragments(-1).await
}

/// Merge multiple clusters into one. All fragment `cluster_id` values are
/// updated to the first ID in the list. The centroid is recomputed and
/// superseding cluster entries are removed from the clusters collection.
#[tauri::command]
pub async fn clustering_merge(
    ids: Vec<i32>,
) -> Result<ClusterView, ClusteringError> {
    if ids.len() < 2 {
        return Err(ClusteringError::InvalidInput(
            "merge requires at least 2 cluster ids".to_string(),
        ));
    }

    let target_label = ids[0];
    let source_labels: Vec<i32> = ids[1..].to_vec();

    let client = get_client();
    let clusters_coll_id = get_collection_id(COLLECTION_CLUSTERS).await?;

    // Collect all fragment IDs that need to be re-assigned.
    let mut all_fragment_ids: Vec<String> = Vec::new();

    // Get target cluster's existing fragments.
    let target_frags = clustering_get_fragments(target_label).await?;
    for frag in &target_frags {
        if let Some(id) = frag.get("id").and_then(|v| v.as_str()) {
            all_fragment_ids.push(id.to_string());
        }
    }

    // Re-assign fragments from source clusters to target.
    for source_label in &source_labels {
        let frags = clustering_get_fragments(*source_label).await?;
        for frag in &frags {
            if let Some(id) = frag.get("id").and_then(|v| v.as_str()) {
                let source_type = frag
                    .get("source_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or(COLLECTION_VAULT);
                let coll_id = get_collection_id(source_type).await?;
                client
                    .update(
                        &coll_id,
                        vec![id.to_string()],
                        None,
                        None,
                        Some(vec![serde_json::json!({ "cluster_id": target_label })]),
                    )
                    .await?;
                all_fragment_ids.push(id.to_string());
            }
        }

        // Remove the source cluster entry.
        let doc_id = format!("cluster_{}", source_label);
        let _ = client.delete(&clusters_coll_id, vec![doc_id]).await;
    }

    // Recompute centroid from all member embeddings.
    let mut member_embeddings: Vec<Vec<f32>> = Vec::new();
    for coll_name in CONTENT_COLLECTIONS {
        let coll_id = match get_collection_id(coll_name).await {
            Ok(id) => id,
            Err(ChromaError::CollectionNotFound(_)) => continue,
            Err(e) => return Err(ClusteringError::Chroma(e)),
        };
        let where_filter = serde_json::json!({ "cluster_id": target_label });
        let result = client
            .get(
                &coll_id,
                None,
                Some(where_filter),
                Some(vec!["embeddings".to_string()]),
            )
            .await?;
        if let Some(embs) = result.embeddings {
            member_embeddings.extend(embs);
        }
    }

    let centroid = compute_centroid(&member_embeddings);

    // Get existing display_label and pinned state from target cluster.
    let target_doc_id = format!("cluster_{}", target_label);
    let existing = client
        .get(
            &clusters_coll_id,
            Some(vec![target_doc_id.clone()]),
            None,
            Some(vec!["metadatas".to_string(), "documents".to_string()]),
        )
        .await?;

    let display_label = existing
        .metadatas
        .as_ref()
        .and_then(|m| m.first())
        .and_then(|m| m.as_ref())
        .and_then(|m| m.get("display_label"))
        .and_then(|v| v.as_str())
        .unwrap_or("Merged cluster")
        .to_string();

    let pinned = existing
        .metadatas
        .as_ref()
        .and_then(|m| m.first())
        .and_then(|m| m.as_ref())
        .and_then(|m| m.get("pinned"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Update the target cluster entry in clusters collection.
    let _ = client.delete(&clusters_coll_id, vec![target_doc_id.clone()]).await;
    client
        .add(
            &clusters_coll_id,
            vec![target_doc_id],
            Some(vec![centroid]),
            Some(vec![display_label.clone()]),
            Some(vec![serde_json::json!({
                "cluster_label": target_label,
                "display_label": display_label,
                "size": all_fragment_ids.len(),
                "pinned": pinned,
                "fragment_ids": all_fragment_ids.join(","),
            })]),
        )
        .await?;

    Ok(ClusterView {
        label: target_label,
        display_label,
        size: all_fragment_ids.len(),
        pinned,
        fragment_ids: all_fragment_ids,
    })
}

/// Split specified fragments out of a cluster into a new cluster.
#[tauri::command]
pub async fn clustering_split(
    cluster_id: i32,
    fragment_ids: Vec<String>,
) -> Result<Vec<ClusterView>, ClusteringError> {
    if fragment_ids.is_empty() {
        return Err(ClusteringError::InvalidInput(
            "fragment_ids must not be empty".to_string(),
        ));
    }

    let client = get_client();
    let clusters_coll_id = get_collection_id(COLLECTION_CLUSTERS).await?;
    let new_label = next_cluster_label().await?;

    // Update fragment metadata: set cluster_id to new_label.
    for frag_id in &fragment_ids {
        for coll_name in CONTENT_COLLECTIONS {
            let coll_id = match get_collection_id(coll_name).await {
                Ok(id) => id,
                Err(ChromaError::CollectionNotFound(_)) => continue,
                Err(e) => return Err(ClusteringError::Chroma(e)),
            };

            // Try to update; if the ID doesn't exist in this collection it
            // will fail silently (Chroma returns success for missing IDs in
            // update).
            let _ = client
                .update(
                    &coll_id,
                    vec![frag_id.clone()],
                    None,
                    None,
                    Some(vec![serde_json::json!({ "cluster_id": new_label })]),
                )
                .await;
        }
    }

    // Update the original cluster entry: remove split fragment IDs and
    // recompute size.
    let orig_doc_id = format!("cluster_{}", cluster_id);
    let existing = client
        .get(
            &clusters_coll_id,
            Some(vec![orig_doc_id.clone()]),
            None,
            Some(vec!["metadatas".to_string()]),
        )
        .await?;

    let orig_meta = existing
        .metadatas
        .as_ref()
        .and_then(|m| m.first())
        .and_then(|m| m.as_ref())
        .cloned()
        .unwrap_or(serde_json::json!({}));

    let orig_fragment_ids: Vec<String> = orig_meta
        .get("fragment_ids")
        .and_then(|v| v.as_str())
        .map(|s| {
            s.split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    let remaining_ids: Vec<String> = orig_fragment_ids
        .into_iter()
        .filter(|id| !fragment_ids.contains(id))
        .collect();

    let orig_display = orig_meta
        .get("display_label")
        .and_then(|v| v.as_str())
        .unwrap_or("Unlabeled")
        .to_string();
    let orig_pinned = orig_meta
        .get("pinned")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Recompute original cluster centroid.
    let mut orig_embeddings: Vec<Vec<f32>> = Vec::new();
    for coll_name in CONTENT_COLLECTIONS {
        let coll_id = match get_collection_id(coll_name).await {
            Ok(id) => id,
            Err(ChromaError::CollectionNotFound(_)) => continue,
            Err(e) => return Err(ClusteringError::Chroma(e)),
        };
        let where_filter = serde_json::json!({ "cluster_id": cluster_id });
        let result = client
            .get(
                &coll_id,
                None,
                Some(where_filter),
                Some(vec!["embeddings".to_string()]),
            )
            .await?;
        if let Some(embs) = result.embeddings {
            orig_embeddings.extend(embs);
        }
    }
    let orig_centroid = compute_centroid(&orig_embeddings);

    // Update original cluster.
    let _ = client.delete(&clusters_coll_id, vec![orig_doc_id.clone()]).await;
    client
        .add(
            &clusters_coll_id,
            vec![orig_doc_id],
            Some(vec![orig_centroid]),
            Some(vec![orig_display.clone()]),
            Some(vec![serde_json::json!({
                "cluster_label": cluster_id,
                "display_label": orig_display,
                "size": remaining_ids.len(),
                "pinned": orig_pinned,
                "fragment_ids": remaining_ids.join(","),
            })]),
        )
        .await?;

    // Compute new cluster centroid from split fragments.
    let mut new_embeddings: Vec<Vec<f32>> = Vec::new();
    for coll_name in CONTENT_COLLECTIONS {
        let coll_id = match get_collection_id(coll_name).await {
            Ok(id) => id,
            Err(ChromaError::CollectionNotFound(_)) => continue,
            Err(e) => return Err(ClusteringError::Chroma(e)),
        };
        let where_filter = serde_json::json!({ "cluster_id": new_label });
        let result = client
            .get(
                &coll_id,
                None,
                Some(where_filter),
                Some(vec!["embeddings".to_string()]),
            )
            .await?;
        if let Some(embs) = result.embeddings {
            new_embeddings.extend(embs);
        }
    }
    let new_centroid = compute_centroid(&new_embeddings);

    // Auto-label the new cluster from its first fragment.
    let new_frags = clustering_get_fragments(new_label).await?;
    let new_docs: Vec<Option<String>> = new_frags
        .iter()
        .map(|f| f.get("content").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect();
    let new_display = auto_label(&new_docs);

    // Add new cluster entry.
    let new_doc_id = format!("cluster_{}", new_label);
    client
        .add(
            &clusters_coll_id,
            vec![new_doc_id],
            Some(vec![new_centroid]),
            Some(vec![new_display.clone()]),
            Some(vec![serde_json::json!({
                "cluster_label": new_label,
                "display_label": new_display,
                "size": fragment_ids.len(),
                "pinned": false,
                "fragment_ids": fragment_ids.join(","),
            })]),
        )
        .await?;

    let original_view = ClusterView {
        label: cluster_id,
        display_label: orig_display,
        size: remaining_ids.len(),
        pinned: orig_pinned,
        fragment_ids: remaining_ids,
    };

    let new_view = ClusterView {
        label: new_label,
        display_label: new_display,
        size: fragment_ids.len(),
        pinned: false,
        fragment_ids,
    };

    Ok(vec![original_view, new_view])
}

/// Move a single fragment from one cluster to another, updating metadata on
/// both the content collection and the clusters collection.
#[tauri::command]
pub async fn clustering_move_fragment(
    fragment_id: String,
    from_cluster: i32,
    to_cluster: i32,
) -> Result<(), ClusteringError> {
    let client = get_client();
    let clusters_coll_id = get_collection_id(COLLECTION_CLUSTERS).await?;

    // Update fragment's cluster_id in whichever content collection it belongs to.
    for coll_name in CONTENT_COLLECTIONS {
        let coll_id = match get_collection_id(coll_name).await {
            Ok(id) => id,
            Err(ChromaError::CollectionNotFound(_)) => continue,
            Err(e) => return Err(ClusteringError::Chroma(e)),
        };

        let _ = client
            .update(
                &coll_id,
                vec![fragment_id.clone()],
                None,
                None,
                Some(vec![serde_json::json!({ "cluster_id": to_cluster })]),
            )
            .await;
    }

    // Update source cluster: remove fragment_id from fragment_ids list and
    // decrement size.
    let from_doc_id = format!("cluster_{}", from_cluster);
    let from_existing = client
        .get(
            &clusters_coll_id,
            Some(vec![from_doc_id.clone()]),
            None,
            Some(vec!["metadatas".to_string(), "documents".to_string()]),
        )
        .await?;

    if let Some(meta) = from_existing
        .metadatas
        .as_ref()
        .and_then(|m| m.first())
        .and_then(|m| m.as_ref())
    {
        let mut frag_ids: Vec<String> = meta
            .get("fragment_ids")
            .and_then(|v| v.as_str())
            .map(|s| {
                s.split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();
        frag_ids.retain(|id| id != &fragment_id);

        client
            .update(
                &clusters_coll_id,
                vec![from_doc_id],
                None,
                None,
                Some(vec![serde_json::json!({
                    "size": frag_ids.len(),
                    "fragment_ids": frag_ids.join(","),
                })]),
            )
            .await?;
    }

    // Update target cluster: add fragment_id and increment size.
    let to_doc_id = format!("cluster_{}", to_cluster);
    let to_existing = client
        .get(
            &clusters_coll_id,
            Some(vec![to_doc_id.clone()]),
            None,
            Some(vec!["metadatas".to_string()]),
        )
        .await?;

    if let Some(meta) = to_existing
        .metadatas
        .as_ref()
        .and_then(|m| m.first())
        .and_then(|m| m.as_ref())
    {
        let mut frag_ids: Vec<String> = meta
            .get("fragment_ids")
            .and_then(|v| v.as_str())
            .map(|s| {
                s.split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();
        frag_ids.push(fragment_id);

        client
            .update(
                &clusters_coll_id,
                vec![to_doc_id],
                None,
                None,
                Some(vec![serde_json::json!({
                    "size": frag_ids.len(),
                    "fragment_ids": frag_ids.join(","),
                })]),
            )
            .await?;
    }

    Ok(())
}

/// Rename a cluster's display label.
#[tauri::command]
pub async fn clustering_rename(
    cluster_id: i32,
    label: String,
) -> Result<(), ClusteringError> {
    let client = get_client();
    let clusters_coll_id = get_collection_id(COLLECTION_CLUSTERS).await?;
    let doc_id = format!("cluster_{}", cluster_id);

    client
        .update(
            &clusters_coll_id,
            vec![doc_id],
            None,
            Some(vec![label.clone()]),
            Some(vec![serde_json::json!({ "display_label": label })]),
        )
        .await?;

    Ok(())
}

/// Pin or unpin a cluster label to prevent auto-relabeling on re-clustering.
#[tauri::command]
pub async fn clustering_pin_label(
    cluster_id: i32,
    pinned: bool,
) -> Result<(), ClusteringError> {
    let client = get_client();
    let clusters_coll_id = get_collection_id(COLLECTION_CLUSTERS).await?;
    let doc_id = format!("cluster_{}", cluster_id);

    client
        .update(
            &clusters_coll_id,
            vec![doc_id],
            None,
            None,
            Some(vec![serde_json::json!({ "pinned": pinned })]),
        )
        .await?;

    Ok(())
}

/// Fetch cluster centroids from the clusters collection, project to 2D via
/// UMAP through the Python sidecar, and return normalized positions.
#[tauri::command]
pub async fn clustering_get_positions() -> Result<Vec<ClusterPosition>, ClusteringError> {
    let client = get_client();
    let grpc = get_grpc_client()?;
    let clusters_coll_id = get_collection_id(COLLECTION_CLUSTERS).await?;

    let result = client
        .get(
            &clusters_coll_id,
            None,
            None,
            Some(vec![
                "embeddings".to_string(),
                "metadatas".to_string(),
            ]),
        )
        .await?;

    if result.ids.is_empty() {
        return Ok(Vec::new());
    }

    let embeddings = match result.embeddings {
        Some(ref embs) => embs,
        None => return Ok(Vec::new()),
    };

    let mut centroids: Vec<Vec<f32>> = Vec::new();
    let mut cluster_ids: Vec<String> = Vec::new();
    let mut label_map: std::collections::HashMap<String, i32> = std::collections::HashMap::new();

    for (i, id) in result.ids.iter().enumerate() {
        let centroid = embeddings.get(i).cloned().unwrap_or_default();
        if centroid.is_empty() {
            continue;
        }

        let label = result
            .metadatas
            .as_ref()
            .and_then(|m| m.get(i))
            .and_then(|m| m.as_ref())
            .and_then(|m| m.get("cluster_label"))
            .and_then(|v| v.as_i64())
            .unwrap_or(-1) as i32;

        centroids.push(centroid);
        cluster_ids.push(id.clone());
        label_map.insert(id.clone(), label);
    }

    if centroids.is_empty() {
        return Ok(Vec::new());
    }

    let projected = grpc
        .project_positions(centroids, cluster_ids)
        .await?;

    let positions = projected
        .into_iter()
        .map(|(cluster_id, x, y)| {
            let label = label_map.get(&cluster_id).copied().unwrap_or(-1);
            ClusterPosition { label, x, y }
        })
        .collect();

    Ok(positions)
}
