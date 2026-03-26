use crate::chroma::client::{get_client, ChromaError};
use crate::chroma::collections::{get_collection_id, COLLECTION_APPLE_NOTES};
use crate::chunker;
use crate::fragment::{self, Fragment, SourceType};
use crate::grpc_client::{get_grpc_client, GrpcError};
use crate::markdown;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum SourceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Directory not found: {0}")]
    DirectoryNotFound(String),
    #[error("Invalid data: {0}")]
    InvalidData(String),
    #[error("Chroma error: {0}")]
    Chroma(#[from] ChromaError),
    #[error("gRPC error: {0}")]
    Grpc(#[from] GrpcError),
}

impl Serialize for SourceError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ---------------------------------------------------------------------------
// Result type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppleNotesScanResult {
    pub imported: usize,
    pub files_scanned: usize,
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Collects all `.md` files from a directory (non-recursive).
fn discover_md_files(dir: &Path) -> Result<Vec<PathBuf>, SourceError> {
    if !dir.exists() || !dir.is_dir() {
        return Err(SourceError::DirectoryNotFound(
            dir.to_string_lossy().to_string(),
        ));
    }

    let mut files = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            files.push(path);
        }
    }

    files.sort();
    Ok(files)
}

/// Processes a single markdown note file into fragments.
/// Uses markdown parsing (preserves headings, frontmatter tags) unlike podcasts
/// which use plain text parsing.
fn process_note_file(path: &Path, source_dir: &str) -> Result<Vec<Fragment>, SourceError> {
    let raw_content = fs::read_to_string(path)?;

    if raw_content.trim().is_empty() {
        return Ok(Vec::new());
    }

    let parsed = markdown::parse_markdown(&raw_content);
    let source_path_str = path.to_string_lossy().to_string();
    let chunks = chunker::chunk_markdown(&parsed, &source_path_str);

    let modified_at = path
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .map(|t| {
            let datetime: chrono::DateTime<chrono::Utc> = t.into();
            datetime.to_rfc3339()
        })
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Extract tags from frontmatter if available.
    let tags: Vec<String> = parsed.tags.clone();

    let fragments: Vec<Fragment> = chunks
        .into_iter()
        .enumerate()
        .map(|(idx, chunk)| {
            let hash = fragment::content_hash(&chunk.content);
            let token_count = fragment::estimate_tokens(&chunk.content);
            let id = ulid::Ulid::new().to_string();

            Fragment {
                id,
                content: chunk.content,
                source_type: SourceType::AppleNotes,
                source_path: source_path_str.clone(),
                chunk_index: idx,
                heading_path: chunk.heading_path,
                tags: tags.clone(),
                token_count,
                content_hash: hash,
                modified_at: modified_at.clone(),
                cluster_id: None,
                disposition: fragment::Disposition::Signal,
                highlights: vec![],
                metadata: serde_json::json!({
                    "file_name": file_name,
                    "source_directory": source_dir,
                }),
            }
        })
        .collect();

    Ok(fragments)
}

/// Processes all `.md` files in a directory and returns fragments + stats.
fn import_apple_notes(
    directory: &str,
) -> Result<(Vec<Fragment>, AppleNotesScanResult), SourceError> {
    let dir = Path::new(directory);
    let files = discover_md_files(dir)?;

    tracing::info!(
        "Discovered {} .md files in {}",
        files.len(),
        directory
    );

    let mut all_fragments = Vec::new();
    let mut files_scanned = 0usize;
    let mut errors = Vec::new();

    for file_path in &files {
        match process_note_file(file_path, directory) {
            Ok(fragments) => {
                files_scanned += 1;
                tracing::debug!(
                    "Processed {}: {} chunks",
                    file_path.display(),
                    fragments.len()
                );
                all_fragments.extend(fragments);
            }
            Err(e) => {
                let msg = format!("{}: {}", file_path.display(), e);
                tracing::warn!("Failed to process Apple Note: {}", msg);
                errors.push(msg);
            }
        }
    }

    let result = AppleNotesScanResult {
        imported: all_fragments.len(),
        files_scanned,
        errors,
    };

    tracing::info!(
        "Apple Notes import: {} fragments from {} files, {} errors",
        result.imported,
        result.files_scanned,
        result.errors.len()
    );

    Ok((all_fragments, result))
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Scans a directory of exported Apple Notes (.md files from Obsidian Importer),
/// parses, chunks, embeds, and stores them in Chroma.
#[tauri::command]
pub async fn source_apple_notes_scan(
    path: String,
) -> Result<AppleNotesScanResult, SourceError> {
    let (fragments, result) = tokio::task::spawn_blocking(move || {
        import_apple_notes(&path)
    })
    .await
    .map_err(|e| SourceError::InvalidData(format!("Task join error: {e}")))?
    ?;

    // Embed and store fragments in Chroma.
    if !fragments.is_empty() {
        let grpc = get_grpc_client()?;
        let client = get_client();
        let coll_id = get_collection_id(COLLECTION_APPLE_NOTES).await?;

        let texts: Vec<String> = fragments.iter().map(|f| f.content.clone()).collect();
        let embeddings = grpc.embed_batch(texts).await?;

        let ids: Vec<String> = fragments.iter().map(|f| f.id.clone()).collect();
        let documents: Vec<String> = fragments.iter().map(|f| f.content.clone()).collect();
        let metadatas: Vec<serde_json::Value> = fragments
            .iter()
            .map(fragment::fragment_to_chroma_metadata)
            .collect();

        client
            .add(&coll_id, ids, Some(embeddings), Some(documents), Some(metadatas))
            .await?;

        tracing::info!(
            "Stored {} Apple Notes fragments in Chroma",
            fragments.len()
        );
    }

    Ok(result)
}

/// Checks that a directory exists and counts how many .md files it contains.
#[tauri::command]
pub async fn source_apple_notes_check(
    path: String,
) -> Result<AppleNotesScanResult, SourceError> {
    let dir = Path::new(&path);

    if !dir.exists() || !dir.is_dir() {
        return Err(SourceError::DirectoryNotFound(path));
    }

    let files = discover_md_files(dir)?;
    Ok(AppleNotesScanResult {
        imported: 0,
        files_scanned: files.len(),
        errors: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_md_files() {
        let tmp = std::env::temp_dir().join("apple_notes_test_discover");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        fs::write(tmp.join("note1.md"), "# Note 1\nSome content.").unwrap();
        fs::write(tmp.join("note2.md"), "# Note 2\nMore content.").unwrap();
        fs::write(tmp.join("readme.txt"), "Not a note.").unwrap();
        fs::write(tmp.join("image.png"), "fake image data").unwrap();

        let files = discover_md_files(&tmp).unwrap();
        assert_eq!(files.len(), 2);

        for f in &files {
            assert_eq!(f.extension().and_then(|e| e.to_str()), Some("md"));
        }

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_discover_no_directory() {
        let result = discover_md_files(Path::new("/nonexistent/apple_notes"));
        assert!(result.is_err());
    }

    #[test]
    fn test_process_note_file() {
        let tmp = std::env::temp_dir().join("apple_notes_test_process.md");
        fs::write(
            &tmp,
            "---\ntags:\n  - apple\n---\n# My Note\n\nSome content about things.\n\n## Section\n\nMore details here.",
        )
        .unwrap();

        let fragments = process_note_file(&tmp, "/tmp").unwrap();
        assert!(!fragments.is_empty());

        let f = &fragments[0];
        assert_eq!(f.source_type, SourceType::AppleNotes);
        assert_eq!(f.disposition, fragment::Disposition::Signal);
        assert_eq!(f.metadata["source_directory"], "/tmp");
        assert!(!f.heading_path.is_empty() || fragments.len() == 1);

        let _ = fs::remove_file(&tmp);
    }

    #[test]
    fn test_process_empty_file() {
        let tmp = std::env::temp_dir().join("apple_notes_test_empty.md");
        fs::write(&tmp, "").unwrap();

        let fragments = process_note_file(&tmp, "/tmp").unwrap();
        assert!(fragments.is_empty());

        let _ = fs::remove_file(&tmp);
    }

    #[test]
    fn test_import_apple_notes() {
        let tmp = std::env::temp_dir().join("apple_notes_test_import");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        fs::write(tmp.join("note1.md"), "# First\nContent one.").unwrap();
        fs::write(tmp.join("note2.md"), "# Second\nContent two.").unwrap();

        let (fragments, result) = import_apple_notes(tmp.to_str().unwrap()).unwrap();

        assert_eq!(result.files_scanned, 2);
        assert_eq!(result.imported, 2);
        assert_eq!(fragments.len(), 2);
        assert!(result.errors.is_empty());

        // Verify disposition is Signal for personal notes.
        for f in &fragments {
            assert_eq!(f.disposition, fragment::Disposition::Signal);
        }

        let _ = fs::remove_dir_all(&tmp);
    }
}
