use parking_lot::{Mutex, RwLock};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const CHROMA_PORT: u16 = 8000;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(thiserror::Error, Debug)]
pub enum ChromaError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Collection not found: {0}")]
    CollectionNotFound(String),
    #[error("Server unavailable")]
    ServerUnavailable,
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Deserialization error: {0}")]
    Deserialize(String),
}

impl Serialize for ChromaError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<reqwest::Error> for ChromaError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_connect() {
            ChromaError::ServerUnavailable
        } else {
            ChromaError::Http(err.to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionInfo {
    pub id: String,
    pub name: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub ids: Vec<Vec<String>>,
    pub distances: Option<Vec<Vec<f64>>>,
    pub documents: Option<Vec<Vec<Option<String>>>>,
    pub metadatas: Option<Vec<Vec<Option<serde_json::Value>>>>,
    pub embeddings: Option<Vec<Vec<Vec<f32>>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetResult {
    pub ids: Vec<String>,
    pub documents: Option<Vec<Option<String>>>,
    pub metadatas: Option<Vec<Option<serde_json::Value>>>,
    pub embeddings: Option<Vec<Vec<f32>>>,
}

// ---------------------------------------------------------------------------
// Global singleton
// ---------------------------------------------------------------------------

static CLIENT: RwLock<Option<ChromaClient>> = RwLock::new(None);
static LAST_HEALTH: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

const HEALTH_TTL: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// ChromaClient
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ChromaClient {
    http: Client,
    base_url: String,
    tenant: String,
    database: String,
}

impl ChromaClient {
    pub fn new(base_url: &str) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            tenant: "default_tenant".to_string(),
            database: "default_database".to_string(),
        }
    }

    // -- Helpers ------------------------------------------------------------

    /// Base URL for collection-scoped operations (Chroma v2 API).
    fn collections_url(&self) -> String {
        format!(
            "{}/api/v2/tenants/{}/databases/{}/collections",
            self.base_url, self.tenant, self.database
        )
    }

    // -- API methods --------------------------------------------------------

    /// GET /api/v2/heartbeat
    pub async fn heartbeat(&self) -> Result<i64, ChromaError> {
        let url = format!("{}/api/v2/heartbeat", self.base_url);
        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            return Err(ChromaError::ServerUnavailable);
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ChromaError::Deserialize(e.to_string()))?;

        body.get("nanosecond heartbeat")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ChromaError::Deserialize("missing heartbeat field".to_string()))
    }

    /// POST /api/v2/tenants/{tenant}/databases/{database}/collections — with get_or_create=true
    pub async fn create_collection(
        &self,
        name: &str,
    ) -> Result<CollectionInfo, ChromaError> {
        let url = self.collections_url();

        let body = serde_json::json!({
            "name": name,
            "get_or_create": true,
        });

        let resp = self.http.post(&url).json(&body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ChromaError::Http(format!("{status}: {text}")));
        }

        resp.json::<CollectionInfo>()
            .await
            .map_err(|e| ChromaError::Deserialize(e.to_string()))
    }

    /// GET /api/v2/tenants/{tenant}/databases/{database}/collections/{name}
    pub async fn get_collection(
        &self,
        name: &str,
    ) -> Result<CollectionInfo, ChromaError> {
        let url = format!("{}/{name}", self.collections_url());

        let resp = self.http.get(&url).send().await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ChromaError::CollectionNotFound(name.to_string()));
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ChromaError::Http(format!("{status}: {text}")));
        }

        resp.json::<CollectionInfo>()
            .await
            .map_err(|e| ChromaError::Deserialize(e.to_string()))
    }

    /// GET /api/v2/tenants/{tenant}/databases/{database}/collections
    pub async fn list_collections(&self) -> Result<Vec<CollectionInfo>, ChromaError> {
        let url = self.collections_url();

        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ChromaError::Http(format!("{status}: {text}")));
        }

        resp.json::<Vec<CollectionInfo>>()
            .await
            .map_err(|e| ChromaError::Deserialize(e.to_string()))
    }

    /// POST /api/v2/.../collections/{id}/add
    pub async fn add(
        &self,
        collection_id: &str,
        ids: Vec<String>,
        embeddings: Option<Vec<Vec<f32>>>,
        documents: Option<Vec<String>>,
        metadatas: Option<Vec<serde_json::Value>>,
    ) -> Result<(), ChromaError> {
        if ids.is_empty() {
            return Err(ChromaError::InvalidInput(
                "ids must not be empty".to_string(),
            ));
        }

        let url = format!("{}/{collection_id}/add", self.collections_url());

        let mut body = serde_json::json!({ "ids": ids });

        if let Some(emb) = embeddings {
            body["embeddings"] = serde_json::json!(emb);
        }
        if let Some(docs) = documents {
            body["documents"] = serde_json::json!(docs);
        }
        if let Some(meta) = metadatas {
            body["metadatas"] = serde_json::json!(meta);
        }

        let resp = self.http.post(&url).json(&body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ChromaError::Http(format!("{status}: {text}")));
        }

        Ok(())
    }

    /// POST /api/v2/.../collections/{id}/query
    pub async fn query(
        &self,
        collection_id: &str,
        query_texts: Option<Vec<String>>,
        query_embeddings: Option<Vec<Vec<f32>>>,
        n_results: usize,
        where_filter: Option<serde_json::Value>,
    ) -> Result<QueryResult, ChromaError> {
        let url = format!("{}/{collection_id}/query", self.collections_url());

        let mut body = serde_json::json!({ "n_results": n_results });

        if let Some(texts) = query_texts {
            body["query_texts"] = serde_json::json!(texts);
        }
        if let Some(emb) = query_embeddings {
            body["query_embeddings"] = serde_json::json!(emb);
        }
        if let Some(filter) = where_filter {
            body["where"] = filter;
        }

        let resp = self.http.post(&url).json(&body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ChromaError::Http(format!("{status}: {text}")));
        }

        resp.json::<QueryResult>()
            .await
            .map_err(|e| ChromaError::Deserialize(e.to_string()))
    }

    /// POST /api/v2/.../collections/{id}/delete
    pub async fn delete(
        &self,
        collection_id: &str,
        ids: Vec<String>,
    ) -> Result<(), ChromaError> {
        let url = format!("{}/{collection_id}/delete", self.collections_url());

        let body = serde_json::json!({ "ids": ids });

        let resp = self.http.post(&url).json(&body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ChromaError::Http(format!("{status}: {text}")));
        }

        Ok(())
    }

    /// POST /api/v2/.../collections/{id}/get
    pub async fn get(
        &self,
        collection_id: &str,
        ids: Option<Vec<String>>,
        where_filter: Option<serde_json::Value>,
        include: Option<Vec<String>>,
    ) -> Result<GetResult, ChromaError> {
        let url = format!("{}/{collection_id}/get", self.collections_url());

        let mut body = serde_json::json!({});

        if let Some(ids) = ids {
            body["ids"] = serde_json::json!(ids);
        }
        if let Some(filter) = where_filter {
            body["where"] = filter;
        }
        if let Some(inc) = include {
            body["include"] = serde_json::json!(inc);
        }

        let resp = self.http.post(&url).json(&body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ChromaError::Http(format!("{status}: {text}")));
        }

        resp.json::<GetResult>()
            .await
            .map_err(|e| ChromaError::Deserialize(e.to_string()))
    }

    /// GET /api/v2/.../collections/{id}/count
    pub async fn count(&self, collection_id: &str) -> Result<usize, ChromaError> {
        let url = format!("{}/{collection_id}/count", self.collections_url());

        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ChromaError::Http(format!("{status}: {text}")));
        }

        resp.json::<usize>()
            .await
            .map_err(|e| ChromaError::Deserialize(e.to_string()))
    }

    /// POST /api/v2/.../collections/{id}/update
    pub async fn update(
        &self,
        collection_id: &str,
        ids: Vec<String>,
        embeddings: Option<Vec<Vec<f32>>>,
        documents: Option<Vec<String>>,
        metadatas: Option<Vec<serde_json::Value>>,
    ) -> Result<(), ChromaError> {
        if ids.is_empty() {
            return Err(ChromaError::InvalidInput(
                "ids must not be empty".to_string(),
            ));
        }

        let url = format!("{}/{collection_id}/update", self.collections_url());

        let mut body = serde_json::json!({ "ids": ids });

        if let Some(emb) = embeddings {
            body["embeddings"] = serde_json::json!(emb);
        }
        if let Some(docs) = documents {
            body["documents"] = serde_json::json!(docs);
        }
        if let Some(meta) = metadatas {
            body["metadatas"] = serde_json::json!(meta);
        }

        let resp = self.http.post(&url).json(&body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ChromaError::Http(format!("{status}: {text}")));
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Global accessors
// ---------------------------------------------------------------------------

/// Returns a `ChromaClient` connected to `127.0.0.1:CHROMA_PORT`.
/// Lazily initializes the global singleton on first call.
pub fn get_client() -> ChromaClient {
    get_client_with_port(CHROMA_PORT)
}

/// Returns a `ChromaClient` connected to the given port.
/// If a client already exists, it is returned regardless of port — call
/// `reset_client()` first to change the port.
pub fn get_client_with_port(port: u16) -> ChromaClient {
    {
        let guard = CLIENT.read();
        if let Some(ref c) = *guard {
            return c.clone();
        }
    }

    let mut guard = CLIENT.write();
    // Double-check after acquiring write lock.
    if let Some(ref c) = *guard {
        return c.clone();
    }

    let client = ChromaClient::new(&format!("http://127.0.0.1:{port}"));
    *guard = Some(client.clone());
    client
}

/// Clears the global singleton so the next `get_client*` call creates a fresh
/// instance.
pub fn reset_client() {
    let mut guard = CLIENT.write();
    *guard = None;
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Health check with a 5-second TTL cache.
#[tauri::command]
pub async fn chroma_health_check() -> Result<bool, ChromaError> {
    // Check cache first.
    {
        let cache = LAST_HEALTH.lock();
        if let Some((instant, healthy)) = *cache {
            if instant.elapsed() < HEALTH_TTL {
                return Ok(healthy);
            }
        }
    }

    let client = get_client();
    let healthy = client.heartbeat().await.is_ok();

    {
        let mut cache = LAST_HEALTH.lock();
        *cache = Some((Instant::now(), healthy));
    }

    Ok(healthy)
}
