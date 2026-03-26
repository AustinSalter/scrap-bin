//! URL Content Extraction
//!
//! Fetches article content from URLs and extracts readable text.
//! Used by Twitter URL expansion and Chrome bookmark import.

use reqwest::Client;
use scraper::{Html, Selector};
use std::sync::OnceLock;
use std::time::Duration;
use tracing::{debug, warn};

/// Extracted article content from a URL
#[derive(Debug, Clone)]
pub struct ArticleContent {
    pub title: Option<String>,
    pub text: String,
    pub word_count: usize,
}

/// URLs to skip (images, video, social profiles — no article text)
const SKIP_EXTENSIONS: &[&str] = &[
    ".jpg", ".jpeg", ".png", ".gif", ".webp", ".svg", ".bmp",
    ".mp4", ".webm", ".mov", ".avi",
    ".mp3", ".wav", ".ogg",
    ".pdf", ".zip", ".tar",
];

const SKIP_DOMAINS: &[&str] = &[
    "twitter.com", "x.com", "instagram.com", "facebook.com",
    "tiktok.com", "youtube.com", "youtu.be",
];

/// Maximum response body size (5 MB)
const MAX_BODY_BYTES: usize = 5 * 1024 * 1024;

/// Shared HTTP client (reused across calls to avoid per-request TLS negotiation)
static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

fn get_http_client() -> &'static Client {
    HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent("Mozilla/5.0 (compatible; Scrapbin/0.1)")
            .build()
            .unwrap_or_else(|_| Client::new())
    })
}

/// Check if a URL should be skipped (media, social profiles, etc.)
fn should_skip(url: &str) -> bool {
    let lower = url.to_lowercase();

    for ext in SKIP_EXTENSIONS {
        if lower.ends_with(ext) {
            return true;
        }
    }

    if let Ok(parsed) = url::Url::parse(&lower) {
        if let Some(host) = parsed.host_str() {
            for domain in SKIP_DOMAINS {
                if host == *domain || host.ends_with(&format!(".{}", domain)) {
                    return true;
                }
            }
        }
    }

    false
}

/// Fetch and extract article content from a URL.
///
/// Returns `None` if the URL should be skipped, fetch fails, or no meaningful text found.
pub async fn fetch_article_content(url: &str) -> Option<ArticleContent> {
    if should_skip(url) {
        debug!(url = %url, "Skipping URL (filtered)");
        return None;
    }

    let client = get_http_client();

    let resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(e) => {
            warn!(url = %url, error = %e, "Failed to fetch URL");
            return None;
        }
    };

    if !resp.status().is_success() {
        debug!(url = %url, status = %resp.status(), "Non-success status");
        return None;
    }

    // Only process HTML responses — skip if content-type is missing or non-HTML
    let is_html = resp
        .headers()
        .get("content-type")
        .and_then(|ct| ct.to_str().ok())
        .map(|s| s.contains("text/html") || s.contains("application/xhtml"))
        .unwrap_or(false);

    if !is_html {
        debug!(url = %url, "Non-HTML or missing content-type, skipping");
        return None;
    }

    // Reject oversized responses early via Content-Length header
    if let Some(len) = resp.content_length() {
        if len > MAX_BODY_BYTES as u64 {
            debug!(url = %url, bytes = len, "Response too large, skipping");
            return None;
        }
    }

    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            warn!(url = %url, error = %e, "Failed to read response body");
            return None;
        }
    };

    // Double-check actual body size (Content-Length can be absent or wrong)
    if bytes.len() > MAX_BODY_BYTES {
        debug!(url = %url, bytes = bytes.len(), "Response body too large, skipping");
        return None;
    }

    let html_text = String::from_utf8_lossy(&bytes);

    extract_article_from_html(&html_text)
}

/// Extract article content from raw HTML.
pub fn extract_article_from_html(html: &str) -> Option<ArticleContent> {
    let document = Html::parse_document(html);

    // Extract title
    let title = Selector::parse("title")
        .ok()
        .and_then(|sel| document.select(&sel).next())
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|t| !t.is_empty());

    // Try article, then main, then largest p-dense div
    let text = extract_by_selector(&document, "article")
        .or_else(|| extract_by_selector(&document, "main"))
        .or_else(|| extract_by_selector(&document, "[role=\"main\"]"))
        .or_else(|| extract_largest_text_block(&document));

    let text = text?;

    // Clean up whitespace
    let cleaned = normalize_whitespace(&text);

    if cleaned.len() < 100 {
        debug!("Extracted text too short ({}), skipping", cleaned.len());
        return None;
    }

    let word_count = cleaned.split_whitespace().count();

    Some(ArticleContent {
        title,
        text: cleaned,
        word_count,
    })
}

/// Extract text from the first matching CSS selector
fn extract_by_selector(document: &Html, selector_str: &str) -> Option<String> {
    let selector = Selector::parse(selector_str).ok()?;
    let element = document.select(&selector).next()?;

    // Collect all text nodes, skipping script/style/nav
    let text = collect_visible_text(element);

    if text.trim().len() >= 100 {
        Some(text)
    } else {
        None
    }
}

/// Find the div/section with the most paragraph text
fn extract_largest_text_block(document: &Html) -> Option<String> {
    let container_sel = Selector::parse("div, section").ok()?;
    let p_sel = Selector::parse("p").ok()?;

    let mut best_text = String::new();
    let mut best_len = 0;

    for container in document.select(&container_sel) {
        let p_count = container.select(&p_sel).count();
        if p_count < 2 {
            continue;
        }

        let text = collect_visible_text(container);
        let text_len = text.len();

        if text_len > best_len {
            best_len = text_len;
            best_text = text;
        }
    }

    if best_text.len() >= 100 {
        Some(best_text)
    } else {
        None
    }
}

/// Collect visible text from an element, skipping script/style/nav/header/footer
fn collect_visible_text(element: scraper::ElementRef) -> String {
    let skip_tags = ["script", "style", "nav", "header", "footer", "aside", "noscript"];

    let mut parts: Vec<String> = Vec::new();

    for node in element.descendants() {
        match node.value() {
            scraper::Node::Element(el) => {
                let tag = el.name();
                if skip_tags.contains(&tag) {
                    // Skip this subtree (handled by skipping text nodes under it)
                    continue;
                }
            }
            scraper::Node::Text(text) => {
                // Check if any ancestor is a skip tag
                let mut skip = false;
                let mut parent = node.parent();
                while let Some(p) = parent {
                    if let Some(el) = p.value().as_element() {
                        if skip_tags.contains(&el.name()) {
                            skip = true;
                            break;
                        }
                    }
                    parent = p.parent();
                }

                if !skip {
                    let t = text.trim();
                    if !t.is_empty() {
                        parts.push(t.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    parts.join(" ")
}

/// Normalize whitespace: collapse runs of spaces/newlines, trim
fn normalize_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev_was_space = false;

    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_was_space {
                result.push(' ');
                prev_was_space = true;
            }
        } else {
            result.push(ch);
            prev_was_space = false;
        }
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_skip() {
        assert!(should_skip("https://example.com/photo.jpg"));
        assert!(should_skip("https://twitter.com/user"));
        assert!(should_skip("https://youtube.com/watch?v=abc"));
        assert!(!should_skip("https://example.com/article"));
        assert!(!should_skip("https://blog.example.com/post/123"));
    }

    #[test]
    fn test_normalize_whitespace() {
        assert_eq!(normalize_whitespace("  hello   world  "), "hello world");
        assert_eq!(normalize_whitespace("a\n\n\nb"), "a b");
    }

    #[test]
    fn test_extract_article_from_html() {
        let html = r#"
        <html>
        <head><title>Test Article</title></head>
        <body>
        <article>
            <p>This is a test article with enough content to pass the minimum length threshold.
            It contains multiple sentences and paragraphs to simulate real article content.
            The content extractor should be able to find and extract this text block successfully.</p>
        </article>
        </body>
        </html>"#;

        let result = extract_article_from_html(html);
        assert!(result.is_some());
        let article = result.unwrap();
        assert_eq!(article.title.as_deref(), Some("Test Article"));
        assert!(article.text.contains("test article"));
        assert!(article.word_count > 10);
    }

    #[test]
    fn test_extract_skips_script_style() {
        let html = r#"
        <html>
        <body>
        <article>
            <script>var x = "should not appear";</script>
            <style>.hidden { display: none; }</style>
            <p>This is the actual visible content that should be extracted from the article.
            It needs to be long enough to pass the minimum length threshold of 100 characters.</p>
        </article>
        </body>
        </html>"#;

        let result = extract_article_from_html(html);
        assert!(result.is_some());
        let text = result.unwrap().text;
        assert!(!text.contains("should not appear"));
        assert!(!text.contains("display: none"));
        assert!(text.contains("actual visible content"));
    }
}
