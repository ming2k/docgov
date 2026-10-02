use pulldown_cmark::{Event, Options, Parser, Tag};
use serde_yaml::Value;
use std::ops::Range;

#[derive(Debug, Clone)]
pub struct MarkdownDocument {
    pub content: String,
    pub line_starts: Vec<usize>,
    pub frontmatter: Option<Frontmatter>,
    pub links: Vec<MarkdownLink>,
}

#[derive(Debug, Clone)]
pub struct Frontmatter {
    pub raw: String,
    pub value: Value,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone)]
pub struct MarkdownLink {
    pub destination: String,
    pub text: String,
    pub line: usize,
    pub column: usize,
    pub byte_range: Range<usize>,
    pub line_snippet: String,
}

impl MarkdownDocument {
    pub fn parse(content: &str) -> Self {
        let line_starts = compute_line_starts(content);
        let frontmatter = extract_frontmatter(content, &line_starts);
        let links = extract_links(content, &line_starts);

        Self {
            content: content.to_string(),
            line_starts,
            frontmatter,
            links,
        }
    }

    pub fn offset_to_line_col(&self, offset: usize) -> (usize, usize) {
        offset_to_line_col(&self.line_starts, offset)
    }

    pub fn get_line(&self, line: usize) -> Option<&str> {
        if line == 0 || line > self.line_starts.len() {
            return None;
        }
        let start = self.line_starts[line - 1];
        let end = if line < self.line_starts.len() {
            self.line_starts[line]
        } else {
            self.content.len()
        };
        Some(self.content[start..end].trim_end_matches(&['\r', '\n'][..]))
    }
}

fn compute_line_starts(content: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, b) in content.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

fn offset_to_line_col(line_starts: &[usize], offset: usize) -> (usize, usize) {
    match line_starts.binary_search(&offset) {
        Ok(idx) => (idx + 1, 1),
        Err(idx) => {
            let line = idx; // 1-indexed
            let col = offset - line_starts[idx - 1] + 1;
            (line, col)
        }
    }
}

fn extract_frontmatter(content: &str, line_starts: &[usize]) -> Option<Frontmatter> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }

    // Find the end of the first line
    let first_line_end = content.find('\n')?;
    if content[..first_line_end].trim() != "---" {
        return None;
    }

    // Find the closing '---'
    let rest = &content[first_line_end + 1..];
    let mut search_pos = 0;
    while let Some(pos) = rest[search_pos..].find("---") {
        let abs_pos = search_pos + pos;
        // Verify this '---' is at the start of a line
        let is_line_start = abs_pos == 0 || rest.as_bytes()[abs_pos - 1] == b'\n';
        let line_end = rest[abs_pos..].find('\n').unwrap_or(rest.len() - abs_pos);
        let line_text = rest[abs_pos..abs_pos + line_end].trim();

        if is_line_start && line_text == "---" {
            let raw_yaml = &rest[..abs_pos];
            if let Ok(value) = serde_yaml::from_str::<Value>(raw_yaml) {
                let closing_offset = first_line_end + 1 + abs_pos;
                let (end_line, _) = offset_to_line_col(line_starts, closing_offset);
                return Some(Frontmatter {
                    raw: raw_yaml.to_string(),
                    value,
                    start_line: 1,
                    end_line,
                });
            }
        }
        search_pos = abs_pos + 3;
    }

    None
}

fn extract_links(content: &str, line_starts: &[usize]) -> Vec<MarkdownLink> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(content, options).into_offset_iter();
    let mut links = Vec::new();
    let mut current_link: Option<(String, Range<usize>, String)> = None;

    for (event, range) in parser {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                current_link = Some((dest_url.to_string(), range, String::new()));
            }
            Event::Text(t) => {
                if let Some((_, _, ref mut text)) = current_link {
                    text.push_str(&t);
                }
            }
            Event::Code(c) => {
                if let Some((_, _, ref mut text)) = current_link {
                    text.push_str(&c);
                }
            }
            Event::End(pulldown_cmark::TagEnd::Link) => {
                if let Some((dest, link_range, text)) = current_link.take() {
                    let (line, column) = offset_to_line_col(line_starts, link_range.start);
                    let line_snippet = get_line_snippet(content, line_starts, line);
                    links.push(MarkdownLink {
                        destination: dest,
                        text,
                        line,
                        column,
                        byte_range: link_range,
                        line_snippet,
                    });
                }
            }
            _ => {}
        }
    }

    links
}

fn get_line_snippet(content: &str, line_starts: &[usize], line: usize) -> String {
    if line == 0 || line > line_starts.len() {
        return String::new();
    }
    let start = line_starts[line - 1];
    let end = if line < line_starts.len() {
        line_starts[line]
    } else {
        content.len()
    };
    content[start..end]
        .trim_end_matches(&['\r', '\n'][..])
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_frontmatter() {
        let md = r#"---
id: ADR-0001
title: Test Decision
status: accepted
date: 2026-08-25
---

# Title
Some content
"#;
        let doc = MarkdownDocument::parse(md);
        assert!(doc.frontmatter.is_some());
        let fm = doc.frontmatter.unwrap();
        assert_eq!(fm.start_line, 1);
        assert_eq!(fm.end_line, 6);
        assert_eq!(fm.value["id"], "ADR-0001");
        assert_eq!(fm.value["status"], "accepted");
    }

    #[test]
    fn test_extract_links_with_positions() {
        let md = r#"# Welcome

Check out [Internal Setup](../dev/setup.md) for details.
Also see [Website](https://example.com).
"#;
        let doc = MarkdownDocument::parse(md);
        assert_eq!(doc.links.len(), 2);

        let link1 = &doc.links[0];
        assert_eq!(link1.text, "Internal Setup");
        assert_eq!(link1.destination, "../dev/setup.md");
        assert_eq!(link1.line, 3);
        assert!(link1.line_snippet.contains("Check out [Internal Setup]"));

        let link2 = &doc.links[1];
        assert_eq!(link2.text, "Website");
        assert_eq!(link2.destination, "https://example.com");
        assert_eq!(link2.line, 4);
    }
}
