use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum StateError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Config error: {0}")]
    Config(#[from] crate::config::ConfigError),
}

impl Serialize for StateError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ---------------------------------------------------------------------------
// Global mutex for serializing state access
// ---------------------------------------------------------------------------

static INDEX_STATE: Mutex<Option<IndexState>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IndexState {
    /// Map from relative file path to its tracked state.
    pub files: HashMap<String, FileState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileState {
    /// SHA-256 hex digest of the file contents.
    pub content_hash: String,
    /// Chroma document IDs produced when this file was last indexed.
    pub chunk_ids: Vec<String>,
    /// ISO 8601 timestamp of the last successful indexing.
    pub last_indexed: String,
    /// Number of chunks produced from this file.
    pub chunk_count: usize,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Loads the incremental index state from disk into the global mutex.
///
/// Returns a default (empty) state if the file does not exist yet.
fn load_from_disk() -> Result<IndexState, StateError> {
    let path = crate::config::index_state_path()?;

    if !path.exists() {
        tracing::debug!("Index state file not found, returning default");
        return Ok(IndexState::default());
    }

    let data = fs::read_to_string(&path)?;
    let state: IndexState = serde_json::from_str(&data)?;
    tracing::debug!("Loaded index state with {} file entries", state.files.len());
    Ok(state)
}

/// Persists the index state to disk.
fn save_to_disk(state: &IndexState) -> Result<(), StateError> {
    let path = crate::config::index_state_path()?;
    let data = serde_json::to_string_pretty(state)?;
    fs::write(&path, data)?;
    tracing::debug!("Saved index state with {} file entries", state.files.len());
    Ok(())
}

/// Acquire the global index state lock and run `f` with mutable access.
/// The state is loaded from disk on first access and saved back after
/// mutation. This serializes all state access, preventing race conditions.
pub fn with_state<F, R>(f: F) -> Result<R, StateError>
where
    F: FnOnce(&mut IndexState) -> Result<R, StateError>,
{
    let mut guard = INDEX_STATE.lock();
    let state = guard.get_or_insert_with(|| {
        load_from_disk().unwrap_or_default()
    });
    let result = f(state)?;
    save_to_disk(state)?;
    Ok(result)
}

/// Like `with_state`, but does NOT flush to disk after mutation.
/// Use this in hot loops (e.g. vault indexing) and call `flush_state()`
/// once at the end to batch the disk write.
pub fn with_state_no_flush<F, R>(f: F) -> Result<R, StateError>
where
    F: FnOnce(&mut IndexState) -> Result<R, StateError>,
{
    let mut guard = INDEX_STATE.lock();
    let state = guard.get_or_insert_with(|| {
        load_from_disk().unwrap_or_default()
    });
    f(state)
}

/// Flush the in-memory index state to disk.
pub fn flush_state() -> Result<(), StateError> {
    let guard = INDEX_STATE.lock();
    if let Some(state) = guard.as_ref() {
        save_to_disk(state)?;
    }
    Ok(())
}

/// Read-only access to the index state (still acquires the lock).
pub fn with_state_read<F, R>(f: F) -> Result<R, StateError>
where
    F: FnOnce(&IndexState) -> R,
{
    let mut guard = INDEX_STATE.lock();
    let state = guard.get_or_insert_with(|| {
        load_from_disk().unwrap_or_default()
    });
    Ok(f(state))
}

/// Reads the file at `path` and returns its SHA-256 hex digest.
pub fn compute_file_hash(path: &Path) -> Result<String, StateError> {
    let data = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// Returns `true` if the file should be re-indexed — either because it is not
/// yet tracked in the state, or because its content hash has changed.
pub fn file_needs_reindex(state: &IndexState, relative_path: &str, current_hash: &str) -> bool {
    match state.files.get(relative_path) {
        Some(entry) => entry.content_hash != current_hash,
        None => true,
    }
}

/// Inserts or updates the file entry in the index state.
pub fn update_file_state(
    state: &mut IndexState,
    relative_path: String,
    hash: String,
    chunk_ids: Vec<String>,
) {
    let chunk_count = chunk_ids.len();
    let last_indexed = chrono::Utc::now().to_rfc3339();

    state.files.insert(
        relative_path,
        FileState {
            content_hash: hash,
            chunk_ids,
            last_indexed,
            chunk_count,
        },
    );
}

/// Removes a file entry from the index state and returns the old entry.
///
/// The caller can use the returned `FileState` to delete stale chunks from
/// Chroma (via `chunk_ids`).
pub fn remove_file_state(state: &mut IndexState, relative_path: &str) -> Option<FileState> {
    state.files.remove(relative_path)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_needs_reindex_missing() {
        let state = IndexState::default();
        assert!(file_needs_reindex(&state, "notes/test.md", "abc123"));
    }

    #[test]
    fn test_file_needs_reindex_same_hash() {
        let mut state = IndexState::default();
        update_file_state(
            &mut state,
            "notes/test.md".to_string(),
            "abc123".to_string(),
            vec!["chunk-1".to_string()],
        );
        assert!(!file_needs_reindex(&state, "notes/test.md", "abc123"));
    }

    #[test]
    fn test_file_needs_reindex_different_hash() {
        let mut state = IndexState::default();
        update_file_state(
            &mut state,
            "notes/test.md".to_string(),
            "abc123".to_string(),
            vec!["chunk-1".to_string()],
        );
        assert!(file_needs_reindex(&state, "notes/test.md", "def456"));
    }

    #[test]
    fn test_update_and_remove_file_state() {
        let mut state = IndexState::default();

        update_file_state(
            &mut state,
            "notes/test.md".to_string(),
            "hash1".to_string(),
            vec!["c1".to_string(), "c2".to_string()],
        );

        assert_eq!(state.files.len(), 1);
        let entry = state.files.get("notes/test.md").unwrap();
        assert_eq!(entry.chunk_count, 2);
        assert_eq!(entry.content_hash, "hash1");

        let removed = remove_file_state(&mut state, "notes/test.md");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().chunk_ids, vec!["c1", "c2"]);
        assert!(state.files.is_empty());
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut state = IndexState::default();
        let removed = remove_file_state(&mut state, "does/not/exist.md");
        assert!(removed.is_none());
    }
}
