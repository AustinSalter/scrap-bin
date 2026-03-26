use serde::{Deserialize, Serialize};
use ulid::Ulid;

use super::markdown::{Heading, ParsedNote};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const MAX_CHUNK_TOKENS: usize = 512;
pub const OVERLAP_TOKENS: usize = 50;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: String,
    pub content: String,
    pub source_path: String,
    pub chunk_index: usize,
    pub heading_path: Vec<String>,
    pub token_count: usize,
    pub start_line: usize,
    pub end_line: usize,
}

// ---------------------------------------------------------------------------
// Token estimation
// ---------------------------------------------------------------------------

/// Rough token estimate: ~4 characters per token.
fn estimate_tokens(text: &str) -> usize {
    (text.len() as f64 / 4.0).ceil() as usize
}

// ---------------------------------------------------------------------------
// Text splitting helpers
// ---------------------------------------------------------------------------

/// Split text into paragraphs (double-newline separated).
fn split_paragraphs(text: &str) -> Vec<&str> {
    let mut paragraphs = Vec::new();
    let mut last = 0;

    // Find double-newline boundaries.
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if i + 1 < len && bytes[i] == b'\n' && bytes[i + 1] == b'\n' {
            let segment = &text[last..i];
            if !segment.trim().is_empty() {
                paragraphs.push(segment.trim());
            }
            // Skip past consecutive newlines.
            while i < len && bytes[i] == b'\n' {
                i += 1;
            }
            last = i;
        } else {
            i += 1;
        }
    }

    // Trailing content.
    if last < len {
        let tail = text[last..].trim();
        if !tail.is_empty() {
            paragraphs.push(tail);
        }
    }

    paragraphs
}

/// Split text by sentence boundaries (period followed by a space or end-of-string).
fn split_sentences(text: &str) -> Vec<&str> {
    let mut sentences = Vec::new();
    let mut last = 0;
    let bytes = text.as_bytes();
    let len = bytes.len();

    let mut i = 0;
    while i < len {
        if bytes[i] == b'.' {
            // Sentence ends at period followed by space, newline, or end of string.
            let at_boundary = if i + 1 < len {
                bytes[i + 1] == b' ' || bytes[i + 1] == b'\n'
            } else {
                true
            };

            if at_boundary {
                let segment = &text[last..=i];
                if !segment.trim().is_empty() {
                    sentences.push(segment.trim());
                }
                last = i + 1;
                // Skip the space after the period.
                if last < len && bytes[last] == b' ' {
                    last += 1;
                }
            }
        }
        i += 1;
    }

    // Trailing content that didn't end with a period.
    if last < len {
        let tail = text[last..].trim();
        if !tail.is_empty() {
            sentences.push(tail);
        }
    }

    sentences
}

/// Split a piece of text into fragments that each fit within `max_tokens`.
/// First tries paragraphs, then sentences, and finally hard-truncates.
fn split_to_fit(text: &str, max_tokens: usize) -> Vec<String> {
    if estimate_tokens(text) <= max_tokens {
        return vec![text.to_string()];
    }

    // Try paragraph split first.
    let paragraphs = split_paragraphs(text);
    let mut result = Vec::new();
    let mut buffer = String::new();

    for para in &paragraphs {
        let combined = if buffer.is_empty() {
            para.to_string()
        } else {
            format!("{}\n\n{}", buffer, para)
        };

        if estimate_tokens(&combined) <= max_tokens {
            buffer = combined;
        } else {
            // Flush current buffer if non-empty.
            if !buffer.is_empty() {
                result.push(buffer);
                buffer = String::new();
            }

            // If this single paragraph exceeds limit, split by sentences.
            if estimate_tokens(para) > max_tokens {
                let sentences = split_sentences(para);
                let mut sent_buf = String::new();
                for sent in sentences {
                    let combined_sent = if sent_buf.is_empty() {
                        sent.to_string()
                    } else {
                        format!("{} {}", sent_buf, sent)
                    };

                    if estimate_tokens(&combined_sent) <= max_tokens {
                        sent_buf = combined_sent;
                    } else {
                        if !sent_buf.is_empty() {
                            result.push(sent_buf);
                            sent_buf = String::new();
                        }
                        // If a single sentence still exceeds, hard truncate in a loop.
                        if estimate_tokens(sent) > max_tokens {
                            let char_limit = max_tokens * 4;
                            let mut remaining = sent;
                            while !remaining.is_empty() {
                                let truncated: String = remaining.chars().take(char_limit).collect();
                                // `truncated.len()` is the byte length of the chars we took,
                                // which is the correct offset for slicing the original `&str`.
                                let consumed = truncated.len();
                                result.push(truncated);
                                remaining = &remaining[consumed..];
                                if estimate_tokens(remaining) <= max_tokens {
                                    if !remaining.trim().is_empty() {
                                        sent_buf = remaining.to_string();
                                    }
                                    break;
                                }
                            }
                        } else {
                            sent_buf = sent.to_string();
                        }
                    }
                }
                if !sent_buf.is_empty() {
                    result.push(sent_buf);
                }
            } else {
                buffer = para.to_string();
            }
        }
    }

    if !buffer.is_empty() {
        result.push(buffer);
    }

    result
}

// ---------------------------------------------------------------------------
// Overlap application
// ---------------------------------------------------------------------------

/// Given a list of raw chunk strings, apply OVERLAP_TOKENS overlap by prepending
/// the tail of the previous chunk to each subsequent chunk.
fn apply_overlap(chunks: &[String]) -> Vec<String> {
    if chunks.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::with_capacity(chunks.len());
    result.push(chunks[0].clone());

    for i in 1..chunks.len() {
        let prev = &chunks[i - 1];
        let overlap_chars = OVERLAP_TOKENS * 4; // approximate char count for overlap
        let overlap_text = if prev.len() > overlap_chars {
            // Find a word boundary near the cut point.
            let start = prev.len() - overlap_chars;
            let adjusted = prev[start..]
                .find(' ')
                .map(|pos| start + pos + 1)
                .unwrap_or(start);
            &prev[adjusted..]
        } else {
            prev.as_str()
        };

        result.push(format!("{}\n\n{}", overlap_text.trim(), chunks[i]));
    }

    result
}

// ---------------------------------------------------------------------------
// Heading path computation
// ---------------------------------------------------------------------------

/// Given a list of headings and a line number, return the chain of ancestor heading texts.
fn heading_path_for_line(headings: &[Heading], line: usize) -> Vec<String> {
    // Find the deepest heading whose range contains `line`, then walk up the hierarchy.
    let mut path: Vec<String> = Vec::new();
    let mut current_level: u8 = u8::MAX;

    // Walk headings in reverse to find ancestors.
    for heading in headings.iter().rev() {
        if heading.line_start <= line && heading.level < current_level {
            path.push(heading.text.clone());
            current_level = heading.level;
            if current_level == 1 {
                break;
            }
        }
    }

    path.reverse();
    path
}

// ---------------------------------------------------------------------------
// Public API: Markdown chunking
// ---------------------------------------------------------------------------

pub fn chunk_markdown(parsed: &ParsedNote, source_path: &str) -> Vec<Chunk> {
    let body = &parsed.body;

    if body.trim().is_empty() {
        return Vec::new();
    }

    let lines: Vec<&str> = body.lines().collect();
    let headings = &parsed.headings;

    // Build sections from headings. Each section spans from a heading to the next.
    // If there is content before the first heading, it becomes the first section.
    let mut sections: Vec<(usize, usize, Vec<String>)> = Vec::new(); // (start_line, end_line, heading_path)

    if headings.is_empty() {
        // No headings — entire body is one section.
        sections.push((0, lines.len(), Vec::new()));
    } else {
        // Content before first heading.
        if headings[0].line_start > 0 {
            sections.push((0, headings[0].line_start, Vec::new()));
        }

        // One section per heading.
        for (i, heading) in headings.iter().enumerate() {
            let end = if i + 1 < headings.len() {
                headings[i + 1].line_start
            } else {
                lines.len()
            };

            let path = heading_path_for_line(headings, heading.line_start);
            sections.push((heading.line_start, end, path));
        }
    }

    // Split each section into token-bounded fragments.
    let mut raw_chunks: Vec<(String, usize, usize, Vec<String>)> = Vec::new(); // (content, start_line, end_line, heading_path)

    for (sec_start, sec_end, heading_path) in &sections {
        let section_text: String = lines[*sec_start..*sec_end].join("\n");
        let fragments = split_to_fit(&section_text, MAX_CHUNK_TOKENS);

        // Approximate line ranges for fragments.
        let mut line_cursor = *sec_start;
        for frag in &fragments {
            let frag_lines = frag.lines().count().max(1);
            let frag_end = (line_cursor + frag_lines).min(*sec_end);
            raw_chunks.push((frag.clone(), line_cursor, frag_end, heading_path.clone()));
            line_cursor = frag_end;
        }
    }

    // Separate out the text for overlap application.
    let texts: Vec<String> = raw_chunks.iter().map(|(t, _, _, _)| t.clone()).collect();
    let overlapped = apply_overlap(&texts);

    // Assemble final Chunk objects.
    overlapped
        .into_iter()
        .enumerate()
        .map(|(idx, content)| {
            let (_, start_line, end_line, ref heading_path) = raw_chunks[idx];
            Chunk {
                id: Ulid::new().to_string(),
                content,
                source_path: source_path.to_string(),
                chunk_index: idx,
                heading_path: heading_path.clone(),
                token_count: estimate_tokens(&raw_chunks[idx].0), // token count of original (pre-overlap)
                start_line,
                end_line,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Public API: Plain text chunking
// ---------------------------------------------------------------------------

pub fn chunk_plain_text(text: &str, source_path: &str) -> Vec<Chunk> {
    if text.trim().is_empty() {
        return Vec::new();
    }

    let fragments = split_to_fit(text, MAX_CHUNK_TOKENS);
    let overlapped = apply_overlap(&fragments);

    // Approximate line ranges.
    let lines: Vec<&str> = text.lines().collect();
    let mut line_cursor = 0;

    overlapped
        .into_iter()
        .enumerate()
        .map(|(idx, content)| {
            let frag_lines = fragments[idx].lines().count().max(1);
            let start_line = line_cursor;
            let end_line = (line_cursor + frag_lines).min(lines.len());
            line_cursor = end_line;

            Chunk {
                id: Ulid::new().to_string(),
                content,
                source_path: source_path.to_string(),
                chunk_index: idx,
                heading_path: Vec::new(),
                token_count: estimate_tokens(&fragments[idx]),
                start_line,
                end_line,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::parse_markdown;

    #[test]
    fn test_short_note_single_chunk() {
        let content = "# Title\nA short paragraph.";
        let parsed = parse_markdown(content);
        let chunks = chunk_markdown(&parsed, "test.md");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].source_path, "test.md");
        assert_eq!(chunks[0].chunk_index, 0);
        assert!(!chunks[0].id.is_empty());
    }

    #[test]
    fn test_heading_path_populated() {
        let content = "# Top\n## Sub\nContent under sub heading.";
        let parsed = parse_markdown(content);
        let chunks = chunk_markdown(&parsed, "test.md");
        // The chunk containing "Content under sub heading" should have heading path.
        let last = chunks.last().unwrap();
        assert!(last.heading_path.contains(&"Top".to_string()));
        assert!(last.heading_path.contains(&"Sub".to_string()));
    }

    #[test]
    fn test_plain_text_chunking() {
        let text = "Paragraph one.\n\nParagraph two.\n\nParagraph three.";
        let chunks = chunk_plain_text(text, "twitter.txt");
        assert!(!chunks.is_empty());
        assert!(chunks[0].heading_path.is_empty());
    }

    #[test]
    fn test_token_estimation() {
        // 20 chars => ceil(20/4) = 5 tokens
        assert_eq!(estimate_tokens("12345678901234567890"), 5);
        // 1 char => ceil(1/4) = 1 token
        assert_eq!(estimate_tokens("a"), 1);
    }

    #[test]
    fn test_large_section_is_split() {
        // Create a body large enough to exceed MAX_CHUNK_TOKENS.
        let word = "lorem ";
        let big_paragraph = word.repeat(MAX_CHUNK_TOKENS * 4); // way over limit
        let content = format!("# Heading\n{}", big_paragraph);
        let parsed = parse_markdown(&content);
        let chunks = chunk_markdown(&parsed, "big.md");
        assert!(chunks.len() > 1, "Expected multiple chunks for large content");
    }

    #[test]
    fn test_empty_body() {
        let parsed = parse_markdown("");
        let chunks = chunk_markdown(&parsed, "empty.md");
        assert!(chunks.is_empty());
    }
}
