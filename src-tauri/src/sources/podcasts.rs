use crate::chroma::client::{get_client, ChromaError};
use crate::chroma::collections::{get_collection_id, COLLECTION_PODCASTS};
use crate::fragment::{self, Fragment, SourceType};
use crate::grpc_client::{get_grpc_client, GrpcError};
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
pub struct PodcastImportResult {
    /// Number of fragments produced across all files.
    pub imported: usize,
    /// Number of transcript files processed.
    pub files_processed: usize,
    /// Per-file error messages (non-fatal).
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Supported transcript file extensions.
const SUPPORTED_EXTENSIONS: &[&str] = &["txt", "srt", "vtt"];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// Use shared helpers from fragment module.

// ---------------------------------------------------------------------------
// Transcript format parsers
// ---------------------------------------------------------------------------

/// Parses an SRT file by stripping numeric cue IDs and timestamps.
///
/// SRT format:
/// ```text
/// 1
/// 00:00:01,000 --> 00:00:04,000
/// Speaker text here.
///
/// 2
/// 00:00:05,000 --> 00:00:08,000
/// More text here.
/// ```
fn parse_srt(content: &str) -> String {
    let mut lines_out = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip empty lines.
        if trimmed.is_empty() {
            continue;
        }

        // Skip numeric cue IDs (lines that are just a number).
        if trimmed.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }

        // Skip timestamp lines: "HH:MM:SS,mmm --> HH:MM:SS,mmm"
        if trimmed.contains("-->")
            && trimmed
                .chars()
                .next()
                .map_or(false, |c| c.is_ascii_digit())
        {
            continue;
        }

        lines_out.push(trimmed);
    }

    lines_out.join(" ")
}

/// Parses a WebVTT file by skipping the WEBVTT header and stripping timestamps.
///
/// VTT format:
/// ```text
/// WEBVTT
///
/// 00:00:01.000 --> 00:00:04.000
/// Speaker text here.
///
/// 00:00:05.000 --> 00:00:08.000
/// More text here.
/// ```
fn parse_vtt(content: &str) -> String {
    let mut lines_out = Vec::new();
    let mut past_header = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip the WEBVTT header line and any header metadata that follows.
        if !past_header {
            if trimmed.starts_with("WEBVTT") {
                continue;
            }
            if trimmed.is_empty() {
                past_header = true;
                continue;
            }
            // Skip header metadata lines (e.g., "Kind: captions").
            continue;
        }

        // Skip empty lines.
        if trimmed.is_empty() {
            continue;
        }

        // Skip timestamp lines: "HH:MM:SS.mmm --> HH:MM:SS.mmm"
        if trimmed.contains("-->")
            && trimmed
                .chars()
                .next()
                .map_or(false, |c| c.is_ascii_digit())
        {
            continue;
        }

        // Skip numeric cue identifiers (optional in VTT).
        if trimmed.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }

        lines_out.push(trimmed);
    }

    lines_out.join(" ")
}

/// Reads a plain .txt transcript as-is (trimmed).
fn parse_txt(content: &str) -> String {
    content.trim().to_string()
}

/// Dispatches to the appropriate parser based on file extension.
fn parse_transcript(path: &Path, content: &str) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("srt") => parse_srt(content),
        Some("vtt") => parse_vtt(content),
        _ => parse_txt(content),
    }
}

// Use shared chunker module.
use crate::chunker;

// ---------------------------------------------------------------------------
// File discovery
// ---------------------------------------------------------------------------

/// Collects all supported transcript files from a directory (non-recursive).
fn discover_transcript_files(dir: &Path) -> Result<Vec<PathBuf>, SourceError> {
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

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        if SUPPORTED_EXTENSIONS.contains(&ext) {
            files.push(path);
        }
    }

    files.sort();
    Ok(files)
}

// ---------------------------------------------------------------------------
// Core import logic
// ---------------------------------------------------------------------------

/// Converts a single transcript file into one or more `Fragment`s.
fn process_transcript_file(path: &Path) -> Result<Vec<Fragment>, SourceError> {
    let raw_content = fs::read_to_string(path)?;
    let clean_text = parse_transcript(path, &raw_content);

    if clean_text.trim().is_empty() {
        return Ok(Vec::new());
    }

    let source_path_str = path.to_string_lossy().to_string();
    let chunked = chunker::chunk_plain_text(&clean_text, &source_path_str);
    let modified_at = path
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .map(|t| {
            let datetime: chrono::DateTime<chrono::Utc> = t.into();
            datetime.to_rfc3339()
        })
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let file_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("txt");

    let fragments: Vec<Fragment> = chunked
        .into_iter()
        .enumerate()
        .map(|(idx, chunk)| {
            let hash = fragment::content_hash(&chunk.content);
            let token_count = fragment::estimate_tokens(&chunk.content);
            let id = ulid::Ulid::new().to_string();

            Fragment {
                id,
                content: chunk.content,
                source_type: SourceType::Podcast,
                source_path: source_path_str.clone(),
                chunk_index: idx,
                heading_path: Vec::new(),
                tags: Vec::new(),
                token_count,
                content_hash: hash,
                modified_at: modified_at.clone(),
                cluster_id: None,
                metadata: serde_json::json!({
                    "file_name": file_stem,
                    "format": ext,
                }),
            }
        })
        .collect();

    Ok(fragments)
}

/// Processes all supported transcript files in `directory` and returns
/// fragments alongside import statistics.
fn import_podcasts(directory: &str) -> Result<(Vec<Fragment>, PodcastImportResult), SourceError> {
    let dir = Path::new(directory);
    let files = discover_transcript_files(dir)?;

    tracing::info!(
        "Discovered {} transcript files in {}",
        files.len(),
        directory
    );

    let mut all_fragments = Vec::new();
    let mut files_processed = 0usize;
    let mut errors = Vec::new();

    for file_path in &files {
        match process_transcript_file(file_path) {
            Ok(fragments) => {
                files_processed += 1;
                tracing::debug!(
                    "Processed {}: {} chunks",
                    file_path.display(),
                    fragments.len()
                );
                all_fragments.extend(fragments);
            }
            Err(e) => {
                let msg = format!("{}: {}", file_path.display(), e);
                tracing::warn!("Failed to process transcript: {}", msg);
                errors.push(msg);
            }
        }
    }

    let result = PodcastImportResult {
        imported: all_fragments.len(),
        files_processed,
        errors,
    };

    tracing::info!(
        "Podcast import: {} fragments from {} files, {} errors",
        result.imported,
        result.files_processed,
        result.errors.len()
    );

    Ok((all_fragments, result))
}

// ---------------------------------------------------------------------------
// Tauri command
// ---------------------------------------------------------------------------

/// Reads all supported transcript files (.txt, .srt, .vtt) from `directory`,
/// parses and chunks them, and returns import statistics.
///
/// Actual embedding and Chroma storage happen downstream in the pipeline.
#[tauri::command]
pub async fn source_podcasts_import(
    directory: String,
) -> Result<PodcastImportResult, SourceError> {
    let (fragments, result) = tokio::task::spawn_blocking(move || {
        import_podcasts(&directory)
    })
    .await
    .map_err(|e| SourceError::InvalidData(format!("Task join error: {e}")))?
    ?;

    // Embed and store fragments in Chroma.
    if !fragments.is_empty() {
        let grpc = get_grpc_client()?;
        let client = get_client();
        let coll_id = get_collection_id(COLLECTION_PODCASTS).await?;

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

        tracing::info!("Stored {} podcast fragments in Chroma", fragments.len());
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_srt() {
        let srt = "\
1
00:00:01,000 --> 00:00:04,000
Hello world.

2
00:00:05,000 --> 00:00:08,000
This is a test.

3
00:00:09,000 --> 00:00:12,000
Final line.
";
        let result = parse_srt(srt);
        assert_eq!(result, "Hello world. This is a test. Final line.");
    }

    #[test]
    fn test_parse_vtt() {
        let vtt = "\
WEBVTT
Kind: captions

00:00:01.000 --> 00:00:04.000
Hello world.

00:00:05.000 --> 00:00:08.000
This is a test.
";
        let result = parse_vtt(vtt);
        assert_eq!(result, "Hello world. This is a test.");
    }

    #[test]
    fn test_parse_vtt_with_cue_ids() {
        let vtt = "\
WEBVTT

1
00:00:01.000 --> 00:00:04.000
First cue.

2
00:00:05.000 --> 00:00:08.000
Second cue.
";
        let result = parse_vtt(vtt);
        assert_eq!(result, "First cue. Second cue.");
    }

    #[test]
    fn test_parse_txt() {
        let txt = "  Just plain text with some whitespace.  \n\n";
        let result = parse_txt(txt);
        assert_eq!(result, "Just plain text with some whitespace.");
    }

    #[test]
    fn test_discover_no_directory() {
        let result = discover_transcript_files(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn test_discover_transcript_files() {
        let tmp = std::env::temp_dir().join("podcast_test_discover");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        // Create some test files.
        fs::write(tmp.join("ep1.srt"), "1\n00:00:00,000 --> 00:00:01,000\nHi.").unwrap();
        fs::write(tmp.join("ep2.vtt"), "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nHello.").unwrap();
        fs::write(tmp.join("ep3.txt"), "Plain transcript.").unwrap();
        fs::write(tmp.join("notes.md"), "Not a transcript.").unwrap();

        let files = discover_transcript_files(&tmp).unwrap();
        assert_eq!(files.len(), 3);

        // Verify only supported extensions are included.
        for f in &files {
            let ext = f.extension().and_then(|e| e.to_str()).unwrap();
            assert!(SUPPORTED_EXTENSIONS.contains(&ext));
        }

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_process_srt_file() {
        let tmp = std::env::temp_dir().join("podcast_test_process.srt");
        fs::write(
            &tmp,
            "1\n00:00:00,000 --> 00:00:03,000\nHello world.\n\n2\n00:00:04,000 --> 00:00:07,000\nSecond line.",
        )
        .unwrap();

        let fragments = process_transcript_file(&tmp).unwrap();
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].content, "Hello world. Second line.");
        assert_eq!(fragments[0].source_type, SourceType::Podcast);
        assert_eq!(fragments[0].metadata["format"], "srt");

        let _ = fs::remove_file(&tmp);
    }

    #[test]
    fn test_import_podcasts() {
        let tmp = std::env::temp_dir().join("podcast_test_import");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        fs::write(tmp.join("ep1.txt"), "Episode one transcript.").unwrap();
        fs::write(tmp.join("ep2.txt"), "Episode two transcript.").unwrap();

        let (fragments, result) =
            import_podcasts(tmp.to_str().unwrap()).unwrap();

        assert_eq!(result.files_processed, 2);
        assert_eq!(result.imported, 2);
        assert_eq!(fragments.len(), 2);
        assert!(result.errors.is_empty());

        let _ = fs::remove_dir_all(&tmp);
    }
}
