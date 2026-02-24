use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Generated protobuf code
// ---------------------------------------------------------------------------

pub mod sidecar_proto {
    tonic::include_proto!("sidecar");
}

use sidecar_proto::clustering_service_client::ClusteringServiceClient;
use sidecar_proto::embedding_service_client::EmbeddingServiceClient;
use sidecar_proto::thread_service_client::ThreadServiceClient;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum GrpcError {
    #[error("gRPC transport error: {0}")]
    Transport(String),
    #[error("gRPC status error: {0}")]
    Status(String),
    #[error("Client not initialized")]
    NotInitialized,
}

impl Serialize for GrpcError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<tonic::transport::Error> for GrpcError {
    fn from(err: tonic::transport::Error) -> Self {
        GrpcError::Transport(err.to_string())
    }
}

impl From<tonic::Status> for GrpcError {
    fn from(err: tonic::Status) -> Self {
        GrpcError::Status(err.to_string())
    }
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterResult {
    pub labels: Vec<i32>,
    pub clusters: Vec<ClusterInfoResult>,
    pub noise_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterInfoResult {
    pub label: i32,
    pub size: i32,
    pub centroid: Vec<f32>,
    pub member_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadConnection {
    pub source_cluster_id: String,
    pub target_cluster_id: String,
    pub similarity: f32,
    pub suggested_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthInfo {
    pub model_name: String,
    pub dimension: i32,
    pub ready: bool,
}

// ---------------------------------------------------------------------------
// Global singleton
// ---------------------------------------------------------------------------

static GRPC_CLIENT: RwLock<Option<SidecarGrpcClient>> = RwLock::new(None);

// ---------------------------------------------------------------------------
// SidecarGrpcClient
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SidecarGrpcClient {
    endpoint: String,
}

impl SidecarGrpcClient {
    fn new(port: u16) -> Self {
        Self {
            endpoint: format!("http://127.0.0.1:{port}"),
        }
    }

    /// Embed a single text into a vector.
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, GrpcError> {
        let channel = tonic::transport::Channel::from_shared(self.endpoint.clone())
            .map_err(|e| GrpcError::Transport(e.to_string()))?
            .connect()
            .await?;

        let mut client = EmbeddingServiceClient::new(channel);

        let request = tonic::Request::new(sidecar_proto::EmbedRequest {
            text: text.to_string(),
        });

        let response = client.embed(request).await?;
        let inner = response.into_inner();

        Ok(inner.embedding)
    }

    /// Embed a batch of texts into vectors.
    pub async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, GrpcError> {
        let channel = tonic::transport::Channel::from_shared(self.endpoint.clone())
            .map_err(|e| GrpcError::Transport(e.to_string()))?
            .connect()
            .await?;

        let mut client = EmbeddingServiceClient::new(channel);

        let request = tonic::Request::new(sidecar_proto::EmbedBatchRequest { texts });

        let response = client.embed_batch(request).await?;
        let inner = response.into_inner();

        let embeddings = inner
            .embeddings
            .into_iter()
            .map(|e| e.values)
            .collect();

        Ok(embeddings)
    }

    /// Cluster embeddings using HDBSCAN.
    pub async fn cluster(
        &self,
        embeddings: Vec<Vec<f32>>,
        ids: Vec<String>,
        min_cluster_size: i32,
        min_samples: i32,
    ) -> Result<ClusterResult, GrpcError> {
        let channel = tonic::transport::Channel::from_shared(self.endpoint.clone())
            .map_err(|e| GrpcError::Transport(e.to_string()))?
            .connect()
            .await?;

        let mut client = ClusteringServiceClient::new(channel);

        let proto_embeddings = embeddings
            .into_iter()
            .map(|values| sidecar_proto::Embedding { values })
            .collect();

        let request = tonic::Request::new(sidecar_proto::ClusterRequest {
            embeddings: proto_embeddings,
            ids,
            min_cluster_size,
            min_samples,
        });

        let response = client.cluster(request).await?;
        let inner = response.into_inner();

        let clusters = inner
            .clusters
            .into_iter()
            .map(|c| ClusterInfoResult {
                label: c.label,
                size: c.size,
                centroid: c.centroid,
                member_ids: c.member_ids,
            })
            .collect();

        Ok(ClusterResult {
            labels: inner.labels,
            clusters,
            noise_count: inner.noise_count,
        })
    }

    /// Detect threads between clusters based on centroid similarity.
    pub async fn detect_threads(
        &self,
        centroids: Vec<(String, Vec<f32>, String)>,
        similarity_threshold: f32,
        max_threads: i32,
    ) -> Result<Vec<ThreadConnection>, GrpcError> {
        let channel = tonic::transport::Channel::from_shared(self.endpoint.clone())
            .map_err(|e| GrpcError::Transport(e.to_string()))?
            .connect()
            .await?;

        let mut client = ThreadServiceClient::new(channel);

        let proto_centroids = centroids
            .into_iter()
            .map(|(cluster_id, centroid, label)| sidecar_proto::ClusterCentroid {
                cluster_id,
                centroid,
                label,
            })
            .collect();

        let request = tonic::Request::new(sidecar_proto::ThreadRequest {
            centroids: proto_centroids,
            similarity_threshold,
            max_threads,
        });

        let response = client.detect_threads(request).await?;
        let inner = response.into_inner();

        let threads = inner
            .threads
            .into_iter()
            .map(|t| ThreadConnection {
                source_cluster_id: t.source_cluster_id,
                target_cluster_id: t.target_cluster_id,
                similarity: t.similarity,
                suggested_label: t.suggested_label,
            })
            .collect();

        Ok(threads)
    }

    /// Health check returning model info.
    pub async fn health(&self) -> Result<HealthInfo, GrpcError> {
        let channel = tonic::transport::Channel::from_shared(self.endpoint.clone())
            .map_err(|e| GrpcError::Transport(e.to_string()))?
            .connect()
            .await?;

        let mut client = EmbeddingServiceClient::new(channel);

        let request = tonic::Request::new(sidecar_proto::HealthRequest {});

        let response = client.health(request).await?;
        let inner = response.into_inner();

        Ok(HealthInfo {
            model_name: inner.model_name,
            dimension: inner.dimension,
            ready: inner.ready,
        })
    }
}

// ---------------------------------------------------------------------------
// Global accessors
// ---------------------------------------------------------------------------

/// Initialize the global gRPC client singleton for the given port.
pub fn init_grpc_client(port: u16) {
    let mut guard = GRPC_CLIENT.write();
    *guard = Some(SidecarGrpcClient::new(port));
    tracing::info!("gRPC client initialized on port {port}");
}

/// Returns a clone of the global gRPC client, or `NotInitialized` if
/// `init_grpc_client` has not been called yet.
pub fn get_grpc_client() -> Result<SidecarGrpcClient, GrpcError> {
    let guard = GRPC_CLIENT.read();
    guard.clone().ok_or(GrpcError::NotInitialized)
}

/// Clear the global singleton so it can be re-initialized with a different
/// port.
pub fn reset_grpc_client() {
    let mut guard = GRPC_CLIENT.write();
    *guard = None;
    tracing::debug!("gRPC client reset");
}
