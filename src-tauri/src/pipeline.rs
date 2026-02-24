use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use thiserror::Error;
use ulid::Ulid;

use crate::chroma::client::{get_client, ChromaError};
use crate::chroma::collections::{get_collection_id, COLLECTION_VAULT};
use crate::chunker;
use crate::fragment::{self, Fragment, SourceType};
use crate::grpc_client::{get_grpc_client, GrpcError};
use crate::markdown;
use crate::state;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum PipelineError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Chroma error: {0}")]
    Chroma(#[from] ChromaError),
    #[error("gRPC error: {0}")]
    Grpc(#[from] GrpcError),
    #[error("State error: {0}")]
    State(String),
    #[error("Vault path does not exist: {0}")]
    VaultNotFound(String),
    #[error("File not found: {0}")]
    FileNotFound(String),
}

impl Serialize for PipelineError {
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
pub struct PipelineStats {
    pub total_files_indexed: usize,
    pub total_chunks: usize,
    pub collections: Vec<CollectionStat>,
    pub last_index_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionStat {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexFileResult {
    pub path: String,
    pub chunks_created: usize,
    pub skipped: bool,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sha256_of_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

fn walk_md_files(dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    walk_md_files_recursive(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn walk_md_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        if name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            walk_md_files_recursive(&path, files)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            files.push(path);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Core pipeline function
// ---------------------------------------------------------------------------

/// Process a single markdown file through the full ingestion pipeline:
///
/// 1. Compute relative path from vault_path
/// 2. Compute SHA-256 hash of file contents
/// 3. Check if the file needs reindexing (hash changed)
/// 4. If unchanged, return skipped = true
/// 5. Parse markdown
/// 6. Chunk the parsed content
/// 7. Create Fragment objects
/// 8. Embed via gRPC sidecar
/// 9. Delete old chunks from Chroma
/// 10. Add new chunks + embeddings to Chroma
/// 11. Update index state
pub async fn process_file(
    vault_path: &Path,
    file_path: &Path,
) -> Result<IndexFileResult, PipelineError> {
    let relative_path = file_path
        .strip_prefix(vault_path)
        .unwrap_or(file_path)
        .to_string_lossy()
        .to_string();

    // ---- 1. Read file contents and compute hash ----------------------------
    if !file_path.exists() {
        return Err(PipelineError::FileNotFound(
            file_path.to_string_lossy().to_string(),
        ));
    }

    let content = std::fs::read_to_string(file_path)?;
    let file_hash = sha256_of_bytes(content.as_bytes());

    // ---- 2. Check if reindex is needed -------------------------------------
    let mut index_state = state::load_state()
        .map_err(|e| PipelineError::State(e.to_string()))?;

    if !state::file_needs_reindex(&index_state, &relative_path, &file_hash) {
        return Ok(IndexFileResult {
            path: relative_path,
            chunks_created: 0,
            skipped: true,
        });
    }

    // ---- 3. Parse markdown -------------------------------------------------
    let parsed = markdown::parse_markdown(&content);

    // ---- 4. Chunk the parsed content ---------------------------------------
    let chunks = chunker::chunk_markdown(&parsed, &relative_path);

    if chunks.is_empty() {
        state::update_file_state(
            &mut index_state,
            relative_path.clone(),
            file_hash,
            vec![],
        );
        state::save_state(&index_state)
            .map_err(|e| PipelineError::State(e.to_string()))?;
        return Ok(IndexFileResult {
            path: relative_path,
            chunks_created: 0,
            skipped: false,
        });
    }

    // ---- 5. Create Fragment objects ----------------------------------------
    let now = chrono::Utc::now().to_rfc3339();
    let fragments: Vec<Fragment> = chunks
        .iter()
        .map(|chunk| {
            let content_hash = sha256_of_bytes(chunk.content.as_bytes());
            Fragment {
                id: Ulid::new().to_string(),
                content: chunk.content.clone(),
                source_type: SourceType::Vault,
                source_path: relative_path.clone(),
                chunk_index: chunk.chunk_index,
                heading_path: chunk.heading_path.clone(),
                tags: parsed.tags.clone(),
                token_count: chunk.token_count,
                content_hash,
                modified_at: now.clone(),
                cluster_id: None,
                metadata: serde_json::json!({}),
            }
        })
        .collect();

    // ---- 6. Get embeddings via gRPC sidecar --------------------------------
    let grpc = get_grpc_client()?;
    let texts: Vec<String> = fragments.iter().map(|f| f.content.clone()).collect();
    let embeddings = grpc.embed_batch(texts).await?;

    // ---- 7. Delete old chunks from Chroma ----------------------------------
    let client = get_client();
    let vault_coll_id = get_collection_id(COLLECTION_VAULT).await?;

    // Remove previous chunks for this source_path.
    if let Some(old_state) = index_state.files.get(&relative_path) {
        if !old_state.chunk_ids.is_empty() {
            client
                .delete(&vault_coll_id, old_state.chunk_ids.clone())
                .await?;
        }
    }

    // ---- 8. Add new chunks + embeddings to Chroma --------------------------
    let ids: Vec<String> = fragments.iter().map(|f| f.id.clone()).collect();
    let documents: Vec<String> = fragments.iter().map(|f| f.content.clone()).collect();
    let metadatas: Vec<serde_json::Value> = fragments
        .iter()
        .map(fragment::fragment_to_chroma_metadata)
        .collect();

    client
        .add(
            &vault_coll_id,
            ids.clone(),
            Some(embeddings),
            Some(documents),
            Some(metadatas),
        )
        .await?;

    // ---- 9. Update index state ---------------------------------------------
    let chunks_created = fragments.len();
    state::update_file_state(
        &mut index_state,
        relative_path.clone(),
        file_hash,
        ids,
    );
    state::save_state(&index_state)
        .map_err(|e| PipelineError::State(e.to_string()))?;

    tracing::debug!("Indexed {}: {} chunks", relative_path, chunks_created);

    Ok(IndexFileResult {
        path: relative_path,
        chunks_created,
        skipped: false,
    })
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn pipeline_index_vault(
    vault_path: String,
) -> Result<Vec<IndexFileResult>, PipelineError> {
    let vault = PathBuf::from(&vault_path);
    if !vault.exists() || !vault.is_dir() {
        return Err(PipelineError::VaultNotFound(vault_path));
    }

    let md_files = walk_md_files(&vault)?;
    let mut results: Vec<IndexFileResult> = Vec::with_capacity(md_files.len());

    tracing::info!(
        "Starting vault index: {} .md files in {}",
        md_files.len(),
        vault_path
    );

    for file_path in &md_files {
        match process_file(&vault, file_path).await {
            Ok(result) => results.push(result),
            Err(e) => {
                tracing::error!("Failed to index {}: {}", file_path.display(), e);
                results.push(IndexFileResult {
                    path: file_path
                        .strip_prefix(&vault)
                        .unwrap_or(file_path)
                        .to_string_lossy()
                        .to_string(),
                    chunks_created: 0,
                    skipped: false,
                });
            }
        }
    }

    let indexed_count = results.iter().filter(|r| !r.skipped).count();
    let total_chunks: usize = results.iter().map(|r| r.chunks_created).sum();

    tracing::info!(
        "Vault index complete: {}/{} files indexed, {} total chunks",
        indexed_count,
        results.len(),
        total_chunks
    );

    Ok(results)
}

#[tauri::command]
pub async fn pipeline_index_file(
    vault_path: String,
    file_path: String,
) -> Result<IndexFileResult, PipelineError> {
    let vault = PathBuf::from(&vault_path);
    let file = PathBuf::from(&file_path);

    if !vault.exists() || !vault.is_dir() {
        return Err(PipelineError::VaultNotFound(vault_path));
    }
    if !file.exists() {
        return Err(PipelineError::FileNotFound(file_path));
    }

    process_file(&vault, &file).await
}

#[tauri::command]
pub async fn pipeline_get_stats() -> Result<PipelineStats, PipelineError> {
    let client = get_client();

    let collection_names = &["vault", "twitter", "readwise", "podcasts"];
    let mut collection_stats: Vec<CollectionStat> = Vec::new();
    let mut total_chunks: usize = 0;

    for name in collection_names {
        let count = match get_collection_id(name).await {
            Ok(coll_id) => client.count(&coll_id).await.unwrap_or(0),
            Err(_) => 0,
        };
        total_chunks += count;
        collection_stats.push(CollectionStat {
            name: name.to_string(),
            count,
        });
    }

    let index_state = state::load_state()
        .map_err(|e| PipelineError::State(e.to_string()))?;

    let total_files_indexed = index_state.files.len();

    // Find the most recent index time across all files.
    let last_index_time = index_state
        .files
        .values()
        .map(|f| f.last_indexed.as_str())
        .max()
        .map(|s| s.to_string());

    Ok(PipelineStats {
        total_files_indexed,
        total_chunks,
        collections: collection_stats,
        last_index_time,
    })
}
