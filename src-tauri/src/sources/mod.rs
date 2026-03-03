pub mod twitter;
pub mod readwise;
pub mod podcasts;
pub mod rss;
pub mod apple_notes;

use crate::config;
use crate::fragment::SourceType;
use serde::Serialize;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Dispatch error
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum DispatchError {
    #[error("Config error: {0}")]
    Config(#[from] config::ConfigError),
    #[error("Source not found: {0}")]
    SourceNotFound(String),
    #[error("Unsupported source type: {0}")]
    UnsupportedSourceType(String),
    #[error("Twitter error: {0}")]
    Twitter(#[from] twitter::SourceError),
    #[error("Readwise error: {0}")]
    Readwise(#[from] readwise::SourceError),
    #[error("RSS error: {0}")]
    Rss(#[from] rss::SourceError),
    #[error("Apple Notes error: {0}")]
    AppleNotes(#[from] apple_notes::SourceError),
}

impl Serialize for DispatchError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct TestSourceResult {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncSourceResult {
    pub success: bool,
    pub message: String,
    pub fragments_imported: usize,
}

// ---------------------------------------------------------------------------
// Dispatcher commands
// ---------------------------------------------------------------------------

/// Tests connectivity for a configured source (dispatches by source_type).
#[tauri::command]
pub async fn test_source(source_id: String) -> Result<TestSourceResult, DispatchError> {
    let sources = config::load_sources()?;
    let source = sources
        .iter()
        .find(|s| s.id == source_id)
        .ok_or_else(|| DispatchError::SourceNotFound(source_id.clone()))?
        .clone();

    match source.source_type {
        SourceType::Twitter => {
            let client_id = source
                .config
                .get("client_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if client_id.is_empty() {
                return Ok(TestSourceResult {
                    success: false,
                    message: "Twitter client_id not configured".to_string(),
                });
            }
            let info = twitter::source_twitter_check_connection(Some(client_id)).await?;
            Ok(TestSourceResult {
                success: info.connected,
                message: if info.connected {
                    format!("Connected as @{}", info.username.unwrap_or_default())
                } else {
                    "Not connected — authorization required".to_string()
                },
            })
        }
        SourceType::Readwise => {
            let connected = readwise::source_readwise_check_connection().await?;
            Ok(TestSourceResult {
                success: connected,
                message: if connected {
                    "Readwise API key is valid".to_string()
                } else {
                    "Readwise API key is invalid or missing".to_string()
                },
            })
        }
        SourceType::Rss => {
            let result = rss::source_rss_check_connection(source_id).await?;
            Ok(TestSourceResult {
                success: result.reachable,
                message: if result.reachable {
                    format!(
                        "Feed reachable: {} ({} entries)",
                        result.feed_title.unwrap_or_default(),
                        result.entry_count
                    )
                } else {
                    "Feed is not reachable".to_string()
                },
            })
        }
        SourceType::AppleNotes => {
            let path = source
                .config
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if path.is_empty() {
                return Ok(TestSourceResult {
                    success: false,
                    message: "No path configured for Apple Notes source".to_string(),
                });
            }
            let result = apple_notes::source_apple_notes_check(path).await?;
            Ok(TestSourceResult {
                success: result.files_scanned > 0,
                message: format!("Found {} .md files", result.files_scanned),
            })
        }
        _ => Err(DispatchError::UnsupportedSourceType(
            source.source_type.to_string(),
        )),
    }
}

/// Syncs a configured source (dispatches by source_type).
#[tauri::command]
pub async fn sync_source(source_id: String) -> Result<SyncSourceResult, DispatchError> {
    let sources = config::load_sources()?;
    let source = sources
        .iter()
        .find(|s| s.id == source_id)
        .ok_or_else(|| DispatchError::SourceNotFound(source_id.clone()))?
        .clone();

    match source.source_type {
        SourceType::Twitter => {
            let client_id = source
                .config
                .get("client_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if client_id.is_empty() {
                return Ok(SyncSourceResult {
                    success: false,
                    message: "Twitter client_id not configured".to_string(),
                    fragments_imported: 0,
                });
            }
            let result = twitter::source_twitter_sync(client_id).await?;
            Ok(SyncSourceResult {
                success: true,
                message: format!(
                    "Imported {} bookmarks ({} skipped, {} threads)",
                    result.imported, result.skipped, result.threads_detected
                ),
                fragments_imported: result.imported,
            })
        }
        SourceType::Readwise => {
            let result = readwise::source_readwise_import().await?;
            Ok(SyncSourceResult {
                success: true,
                message: format!(
                    "Imported {} highlights ({} fetched)",
                    result.imported, result.total_fetched
                ),
                fragments_imported: result.imported,
            })
        }
        SourceType::Rss => {
            let result = rss::source_rss_poll(source_id).await?;
            Ok(SyncSourceResult {
                success: true,
                message: format!(
                    "Imported {} fragments from {} ({} entries fetched)",
                    result.imported, result.feed_title, result.entries_fetched
                ),
                fragments_imported: result.imported,
            })
        }
        SourceType::AppleNotes => {
            let path = source
                .config
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if path.is_empty() {
                return Ok(SyncSourceResult {
                    success: false,
                    message: "No path configured for Apple Notes source".to_string(),
                    fragments_imported: 0,
                });
            }
            let result = apple_notes::source_apple_notes_scan(path).await?;
            Ok(SyncSourceResult {
                success: true,
                message: format!(
                    "Imported {} fragments from {} files",
                    result.imported, result.files_scanned
                ),
                fragments_imported: result.imported,
            })
        }
        _ => Err(DispatchError::UnsupportedSourceType(
            source.source_type.to_string(),
        )),
    }
}
