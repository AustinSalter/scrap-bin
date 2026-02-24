use serde::{Deserialize, Serialize};
use thiserror::Error;
use ulid::Ulid;

use crate::chroma::client::{get_client, ChromaError};
use crate::chroma::collections::{get_collection_id, COLLECTION_CLUSTERS, COLLECTION_THREADS};
use crate::grpc_client::{get_grpc_client, GrpcError};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum ThreadError {
    #[error("Chroma error: {0}")]
    Chroma(#[from] ChromaError),
    #[error("gRPC error: {0}")]
    Grpc(#[from] GrpcError),
    #[error("Thread not found: {0}")]
    NotFound(String),
    #[error("No clusters available for thread detection")]
    NoClusters,
}

impl Serialize for ThreadError {
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
pub struct ThreadView {
    pub id: String,
    pub source_cluster: String,
    pub target_cluster: String,
    pub similarity: f32,
    pub label: Option<String>,
    pub confirmed: bool,
    pub dismissed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadParams {
    /// Minimum cosine similarity between cluster centroids to consider a
    /// thread. Default: 0.65.
    pub similarity_threshold: Option<f32>,
    /// Maximum number of threads to return. Default: 50.
    pub max_threads: Option<i32>,
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Detect threads between clusters by computing centroid similarity via the
/// Python sidecar. Persists results into the `threads` Chroma collection.
#[tauri::command]
pub async fn threads_detect(
    params: ThreadParams,
) -> Result<Vec<ThreadView>, ThreadError> {
    let client = get_client();
    let grpc = get_grpc_client()?;

    let similarity_threshold = params.similarity_threshold.unwrap_or(0.65);
    let max_threads = params.max_threads.unwrap_or(50);

    // ---- 1. Fetch cluster centroids from the clusters collection -----------
    let clusters_coll_id = get_collection_id(COLLECTION_CLUSTERS).await?;
    let clusters = client
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

    if clusters.ids.is_empty() {
        return Err(ThreadError::NoClusters);
    }

    // Build centroid tuples: (cluster_id_string, centroid_vec, label).
    let embeddings = clusters.embeddings.as_ref().ok_or(ThreadError::NoClusters)?;
    let metadatas = clusters.metadatas.as_ref();

    let mut centroids: Vec<(String, Vec<f32>, String)> = Vec::new();

    for (i, id) in clusters.ids.iter().enumerate() {
        let centroid = embeddings.get(i).cloned().unwrap_or_default();
        let label = metadatas
            .and_then(|m| m.get(i))
            .and_then(|m| m.as_ref())
            .and_then(|m| m.get("display_label"))
            .and_then(|v| v.as_str())
            .unwrap_or("Unlabeled")
            .to_string();

        // Use the cluster doc ID (e.g. "cluster_0") as the identifier.
        centroids.push((id.clone(), centroid, label));
    }

    // ---- 2. Call Python sidecar for thread detection -----------------------
    let connections = grpc
        .detect_threads(centroids, similarity_threshold, max_threads)
        .await?;

    // ---- 3. Persist to threads collection ----------------------------------
    let threads_coll_id = get_collection_id(COLLECTION_THREADS).await?;

    // Clear previous thread entries.
    let existing = client.get(&threads_coll_id, None, None, None).await?;
    if !existing.ids.is_empty() {
        client.delete(&threads_coll_id, existing.ids).await?;
    }

    let mut views: Vec<ThreadView> = Vec::new();

    for conn in &connections {
        let thread_id = Ulid::new().to_string();
        let suggested_label = if conn.suggested_label.is_empty() {
            None
        } else {
            Some(conn.suggested_label.clone())
        };

        let view = ThreadView {
            id: thread_id.clone(),
            source_cluster: conn.source_cluster_id.clone(),
            target_cluster: conn.target_cluster_id.clone(),
            similarity: conn.similarity,
            label: suggested_label.clone(),
            confirmed: false,
            dismissed: false,
        };

        // Store thread in Chroma. We use the ULID as the document ID.
        // No embedding is needed for threads — they are metadata-only entries.
        // We store a dummy document to satisfy Chroma's requirement.
        client
            .add(
                &threads_coll_id,
                vec![thread_id.clone()],
                None,
                Some(vec![format!(
                    "{} <-> {}",
                    conn.source_cluster_id, conn.target_cluster_id
                )]),
                Some(vec![serde_json::json!({
                    "source_cluster": conn.source_cluster_id,
                    "target_cluster": conn.target_cluster_id,
                    "similarity": conn.similarity,
                    "label": suggested_label.clone().unwrap_or_default(),
                    "confirmed": false,
                    "dismissed": false,
                })]),
            )
            .await?;

        views.push(view);
    }

    tracing::info!("Thread detection complete: {} threads found", views.len());

    Ok(views)
}

/// Return all threads from the `threads` Chroma collection.
#[tauri::command]
pub async fn threads_get_all() -> Result<Vec<ThreadView>, ThreadError> {
    let client = get_client();
    let threads_coll_id = get_collection_id(COLLECTION_THREADS).await?;

    let result = client
        .get(
            &threads_coll_id,
            None,
            None,
            Some(vec!["metadatas".to_string()]),
        )
        .await?;

    let mut views: Vec<ThreadView> = Vec::new();

    if let Some(ref metas) = result.metadatas {
        for (i, meta_opt) in metas.iter().enumerate() {
            let id = result.ids.get(i).cloned().unwrap_or_default();

            if let Some(meta) = meta_opt {
                let source_cluster = meta
                    .get("source_cluster")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let target_cluster = meta
                    .get("target_cluster")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let similarity = meta
                    .get("similarity")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32;
                let label = meta
                    .get("label")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                let confirmed = meta
                    .get("confirmed")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let dismissed = meta
                    .get("dismissed")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                views.push(ThreadView {
                    id,
                    source_cluster,
                    target_cluster,
                    similarity,
                    label,
                    confirmed,
                    dismissed,
                });
            }
        }
    }

    Ok(views)
}

/// Set a display label on a thread.
#[tauri::command]
pub async fn threads_name(
    id: String,
    label: String,
) -> Result<(), ThreadError> {
    let client = get_client();
    let threads_coll_id = get_collection_id(COLLECTION_THREADS).await?;

    client
        .update(
            &threads_coll_id,
            vec![id],
            None,
            None,
            Some(vec![serde_json::json!({ "label": label })]),
        )
        .await?;

    Ok(())
}

/// Mark a thread as confirmed (validated by the user).
#[tauri::command]
pub async fn threads_confirm(id: String) -> Result<(), ThreadError> {
    let client = get_client();
    let threads_coll_id = get_collection_id(COLLECTION_THREADS).await?;

    client
        .update(
            &threads_coll_id,
            vec![id],
            None,
            None,
            Some(vec![serde_json::json!({ "confirmed": true })]),
        )
        .await?;

    Ok(())
}

/// Mark a thread as dismissed (hidden from the default view).
#[tauri::command]
pub async fn threads_dismiss(id: String) -> Result<(), ThreadError> {
    let client = get_client();
    let threads_coll_id = get_collection_id(COLLECTION_THREADS).await?;

    client
        .update(
            &threads_coll_id,
            vec![id],
            None,
            None,
            Some(vec![serde_json::json!({ "dismissed": true })]),
        )
        .await?;

    Ok(())
}
