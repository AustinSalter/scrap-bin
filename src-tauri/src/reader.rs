//! Reader module — Tauri commands for article extraction.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::content_extractor;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum ReaderError {
    #[error("Failed to extract article from URL: {0}")]
    ExtractionFailed(String),
}

impl Serialize for ReaderError {
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
pub struct ExtractedArticle {
    pub url: String,
    pub title: Option<String>,
    pub text: String,
    pub word_count: usize,
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Extract article content from a URL. Returns the extracted text and metadata.
#[tauri::command]
pub async fn extract_article(url: String) -> Result<ExtractedArticle, ReaderError> {
    let article = content_extractor::fetch_article_content(&url)
        .await
        .ok_or_else(|| ReaderError::ExtractionFailed(url.clone()))?;

    Ok(ExtractedArticle {
        url,
        title: article.title,
        text: article.text,
        word_count: article.word_count,
    })
}
