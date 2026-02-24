use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEvent, Debouncer};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum WatcherError {
    #[error("Watcher is already active")]
    AlreadyActive,
    #[error("Watcher is not active")]
    NotActive,
    #[error("Vault path does not exist: {0}")]
    PathNotFound(String),
    #[error("Notify error: {0}")]
    Notify(#[from] notify::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl Serialize for WatcherError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ---------------------------------------------------------------------------
// Event types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileEventType {
    Created,
    Modified,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeEvent {
    pub event_type: FileEventType,
    pub path: String,
    pub absolute_path: String,
    pub file_hash: Option<String>,
    pub timestamp: String,
}

// ---------------------------------------------------------------------------
// VaultWatcher singleton
// ---------------------------------------------------------------------------

struct VaultWatcher {
    _debouncer: Debouncer<notify::RecommendedWatcher>,
    vault_path: PathBuf,
}

static VAULT_WATCHER: RwLock<Option<VaultWatcher>> = RwLock::new(None);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compute SHA-256 hex digest for a file.
fn sha256_file(path: &Path) -> Option<String> {
    let data = fs::read(path).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let result = hasher.finalize();
    Some(format!("{:x}", result))
}

/// Return true if `path` should be ignored (hidden dirs, `.obsidian/`, non-`.md`).
fn should_ignore(path: &Path) -> bool {
    // Must be a .md file (or a directory — we let directories through for recursive watching,
    // but the events we *emit* are filtered to .md only).
    if path.is_file() {
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            return true;
        }
    }

    // Skip any component that starts with `.` (hidden dirs, .obsidian, .trash, etc.)
    for component in path.components() {
        if let std::path::Component::Normal(seg) = component {
            if let Some(s) = seg.to_str() {
                if s.starts_with('.') {
                    return true;
                }
            }
        }
    }

    false
}

/// Classify a debounced event into a `FileEventType`.
fn classify_event(path: &Path) -> FileEventType {
    if !path.exists() {
        FileEventType::Deleted
    } else {
        // notify-debouncer-mini does not distinguish create vs modify;
        // we treat all existing-file events as Modified (the frontend can
        // check its own cache to decide if it was truly new).
        FileEventType::Modified
    }
}

/// Build a `FileChangeEvent` from a raw debounced path.
fn build_change_event(path: &Path, vault_root: &Path) -> Option<FileChangeEvent> {
    if should_ignore(path) {
        return None;
    }

    let event_type = classify_event(path);

    let relative = path
        .strip_prefix(vault_root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let absolute_path = path.to_string_lossy().to_string();

    let file_hash = match event_type {
        FileEventType::Deleted => None,
        _ => sha256_file(path),
    };

    let timestamp = chrono::Utc::now().to_rfc3339();

    Some(FileChangeEvent {
        event_type,
        path: relative,
        absolute_path,
        file_hash,
        timestamp,
    })
}

// ---------------------------------------------------------------------------
// Core start / stop logic
// ---------------------------------------------------------------------------

fn start_watching(app: AppHandle, vault_path: PathBuf) -> Result<(), WatcherError> {
    let mut guard = VAULT_WATCHER.write();
    if guard.is_some() {
        return Err(WatcherError::AlreadyActive);
    }

    if !vault_path.exists() || !vault_path.is_dir() {
        return Err(WatcherError::PathNotFound(
            vault_path.to_string_lossy().to_string(),
        ));
    }

    let vault_root = vault_path.clone();
    let debouncer = new_debouncer(
        Duration::from_millis(300),
        move |result: Result<Vec<DebouncedEvent>, notify::Error>| {
            match result {
                Ok(events) => {
                    let changes: Vec<FileChangeEvent> = events
                        .iter()
                        .filter_map(|ev| build_change_event(&ev.path, &vault_root))
                        .collect();

                    if !changes.is_empty() {
                        if let Err(e) = app.emit("vault-file-changed", &changes) {
                            tracing::error!("Failed to emit vault-file-changed event: {}", e);
                        }
                        tracing::debug!(
                            "Emitted {} file change event(s)",
                            changes.len()
                        );
                    }
                }
                Err(e) => {
                    tracing::error!("File watcher error: {}", e);
                }
            }
        },
    )?;

    // Start watching the vault recursively.
    debouncer
        .watcher()
        .watch(&vault_path, RecursiveMode::Recursive)?;

    tracing::info!("Vault watcher started: {}", vault_path.display());

    *guard = Some(VaultWatcher {
        _debouncer: debouncer,
        vault_path,
    });

    Ok(())
}

/// Stop the active watcher (if any). Called from `main.rs` on shutdown.
pub fn stop_watching() {
    let mut guard = VAULT_WATCHER.write();
    if guard.take().is_some() {
        tracing::info!("Vault watcher stopped");
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn watcher_start(app: AppHandle, vault_path: String) -> Result<(), WatcherError> {
    let path = PathBuf::from(&vault_path);
    start_watching(app, path)
}

#[tauri::command]
pub fn watcher_stop() -> Result<(), WatcherError> {
    let mut guard = VAULT_WATCHER.write();
    match guard.take() {
        Some(_) => {
            tracing::info!("Vault watcher stopped via command");
            Ok(())
        }
        None => Err(WatcherError::NotActive),
    }
}

#[tauri::command]
pub fn watcher_is_active() -> bool {
    VAULT_WATCHER.read().is_some()
}

#[tauri::command]
pub fn watcher_get_vault_path() -> Option<String> {
    VAULT_WATCHER
        .read()
        .as_ref()
        .map(|w| w.vault_path.to_string_lossy().to_string())
}
