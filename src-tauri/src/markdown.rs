use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedNote {
    pub frontmatter: HashMap<String, serde_yaml::Value>,
    pub headings: Vec<Heading>,
    pub links: Vec<WikiLink>,
    pub tags: Vec<String>,
    pub body: String,
    pub word_count: usize,
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub line_start: usize,
    pub line_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiLink {
    pub target: String,
    pub alias: Option<String>,
    pub line: usize,
}

// ---------------------------------------------------------------------------
// Frontmatter parsing
// ---------------------------------------------------------------------------

/// Strip YAML frontmatter delimited by `---` and return (frontmatter map, body).
fn parse_frontmatter(content: &str) -> (HashMap<String, serde_yaml::Value>, String) {
    let trimmed = content.trim_start();

    if !trimmed.starts_with("---") {
        return (HashMap::new(), content.to_string());
    }

    // Find the closing `---` (must be on its own line after the opening one).
    // The opening delimiter is the first line.
    let after_open = match trimmed.strip_prefix("---") {
        Some(rest) => rest,
        None => return (HashMap::new(), content.to_string()),
    };

    // Skip the newline right after the opening `---`.
    let after_open = after_open.strip_prefix('\n').unwrap_or(after_open);

    // Find the closing `---` on its own line.
    let closing_pos = after_open
        .find("\n---")
        .map(|pos| pos + 1); // +1 to point at the first `-`

    let (yaml_str, body) = match closing_pos {
        Some(pos) => {
            let yaml_block = &after_open[..pos - 1]; // exclude the newline before ---
            let rest_start = pos + 3; // skip `---`
            let rest = &after_open[rest_start..];
            // Strip the leading newline from the body if present.
            let rest = rest.strip_prefix('\n').unwrap_or(rest);
            (yaml_block, rest.to_string())
        }
        None => {
            // No closing delimiter found — treat entire content as body.
            return (HashMap::new(), content.to_string());
        }
    };

    let map: HashMap<String, serde_yaml::Value> =
        serde_yaml::from_str(yaml_str).unwrap_or_default();

    (map, body)
}

// ---------------------------------------------------------------------------
// Heading extraction
// ---------------------------------------------------------------------------

fn extract_headings(body: &str) -> Vec<Heading> {
    let lines: Vec<&str> = body.lines().collect();
    let mut headings: Vec<Heading> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('#') {
            continue;
        }

        // Count consecutive `#` characters.
        let level = trimmed.chars().take_while(|&c| c == '#').count();
        if level == 0 || level > 6 {
            continue;
        }

        // Must be followed by a space (ATX heading).
        let rest = &trimmed[level..];
        if !rest.starts_with(' ') && !rest.is_empty() {
            continue;
        }

        let text = rest.trim().to_string();

        headings.push(Heading {
            level: level as u8,
            text,
            line_start: i,
            line_end: 0, // filled in below
        });
    }

    // Fill in `line_end`: each heading's range extends to the start of the next heading
    // (or the end of the document).
    let total_lines = body.lines().count();
    for i in 0..headings.len() {
        headings[i].line_end = if i + 1 < headings.len() {
            headings[i + 1].line_start
        } else {
            total_lines
        };
    }

    headings
}

// ---------------------------------------------------------------------------
// Wiki-link extraction (no regex)
// ---------------------------------------------------------------------------

fn extract_wiki_links(body: &str) -> Vec<WikiLink> {
    let mut links = Vec::new();

    for (line_idx, line) in body.lines().enumerate() {
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i + 1 < len {
            // Look for `[[`
            if bytes[i] == b'[' && bytes[i + 1] == b'[' {
                let start = i + 2;
                // Find the closing `]]`
                if let Some(rel_end) = line[start..].find("]]") {
                    let inner = &line[start..start + rel_end];
                    if !inner.is_empty() {
                        let (target, alias) = if let Some(pipe_pos) = inner.find('|') {
                            (
                                inner[..pipe_pos].to_string(),
                                Some(inner[pipe_pos + 1..].to_string()),
                            )
                        } else {
                            (inner.to_string(), None)
                        };

                        links.push(WikiLink {
                            target,
                            alias,
                            line: line_idx,
                        });
                    }
                    i = start + rel_end + 2; // skip past `]]`
                    continue;
                }
            }
            i += 1;
        }
    }

    links
}

// ---------------------------------------------------------------------------
// Tag extraction
// ---------------------------------------------------------------------------

fn extract_tags(body: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in body.lines() {
        let chars: Vec<char> = line.chars().collect();

        for (i, &ch) in chars.iter().enumerate() {
            if ch != '#' {
                continue;
            }

            // Word boundary check: `#` must be at start of line or preceded by whitespace.
            if i > 0 && !chars[i - 1].is_whitespace() {
                continue;
            }

            // Collect tag body: alphanumeric, hyphen, underscore, slash (nested tags).
            let tag_start = i + 1;
            let mut end = tag_start;
            while end < chars.len() {
                let c = chars[end];
                if c.is_alphanumeric() || c == '-' || c == '_' || c == '/' {
                    end += 1;
                } else {
                    break;
                }
            }

            if end > tag_start {
                let tag: String = chars[tag_start..end].iter().collect();

                // Skip if this looks like a heading (tag is empty after `# ` would have
                // been caught above, but guard against pure-number "tags" from headings).
                // A heading line starts with `# ` so tags extracted mid-line are fine.

                if !seen.contains(&tag) {
                    seen.insert(tag.clone());
                    tags.push(tag);
                }
            }
        }
    }

    tags
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn parse_markdown(content: &str) -> ParsedNote {
    let (frontmatter, body) = parse_frontmatter(content);
    let headings = extract_headings(&body);
    let links = extract_wiki_links(&body);
    let tags = extract_tags(&body);

    let word_count = body.split_whitespace().count();
    let estimated_tokens = (word_count as f64 * 1.3).ceil() as usize;

    ParsedNote {
        frontmatter,
        headings,
        links,
        tags,
        body,
        word_count,
        estimated_tokens,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frontmatter_parsing() {
        let input = "---\ntitle: Hello\ntags:\n  - rust\n  - tauri\n---\n# Body\nSome text.";
        let note = parse_markdown(input);
        assert!(note.frontmatter.contains_key("title"));
        assert_eq!(note.body.starts_with("# Body"), true);
    }

    #[test]
    fn test_no_frontmatter() {
        let input = "# Just a heading\nSome body text here.";
        let note = parse_markdown(input);
        assert!(note.frontmatter.is_empty());
        assert_eq!(note.headings.len(), 1);
        assert_eq!(note.headings[0].level, 1);
        assert_eq!(note.headings[0].text, "Just a heading");
    }

    #[test]
    fn test_wiki_links() {
        let input = "See [[PageA]] and [[PageB|display text]] for details.";
        let note = parse_markdown(input);
        assert_eq!(note.links.len(), 2);
        assert_eq!(note.links[0].target, "PageA");
        assert_eq!(note.links[0].alias, None);
        assert_eq!(note.links[1].target, "PageB");
        assert_eq!(note.links[1].alias, Some("display text".to_string()));
    }

    #[test]
    fn test_tags() {
        let input = "#rust is great\nSome text #tauri and #deep/nested tag.";
        let note = parse_markdown(input);
        assert!(note.tags.contains(&"rust".to_string()));
        assert!(note.tags.contains(&"tauri".to_string()));
        assert!(note.tags.contains(&"deep/nested".to_string()));
    }

    #[test]
    fn test_heading_line_ranges() {
        let input = "# H1\nline1\nline2\n## H2\nline3\n";
        let note = parse_markdown(input);
        assert_eq!(note.headings.len(), 2);
        assert_eq!(note.headings[0].line_start, 0);
        assert_eq!(note.headings[0].line_end, 3);
        assert_eq!(note.headings[1].line_start, 3);
        assert_eq!(note.headings[1].line_end, 5);
    }

    #[test]
    fn test_word_count_and_tokens() {
        let input = "one two three four five";
        let note = parse_markdown(input);
        assert_eq!(note.word_count, 5);
        assert_eq!(note.estimated_tokens, 7); // ceil(5 * 1.3) = 7
    }
}
