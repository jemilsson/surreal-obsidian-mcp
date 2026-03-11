use anyhow::{Context, Result};
use gray_matter::engine::YAML;
use gray_matter::Matter;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;

/// Represents a parsed heading from a markdown document
#[derive(Debug, Clone)]
pub struct ParsedHeading {
    pub level: u8,
    pub title: String,
    pub content: String,
    pub start_offset: usize,
    pub end_offset: usize,
}

/// Represents a parsed markdown document
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    pub frontmatter: HashMap<String, Value>,
    pub content: String, // Content without frontmatter
    pub headings: Vec<ParsedHeading>,
    pub wiki_links: Vec<WikiLink>,
    pub tags: Vec<String>,
}

/// Represents a wiki-style link [[target]] or [[target|alias]]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiLink {
    pub target: String,
    pub alias: Option<String>,
}

/// Parse a markdown document with frontmatter, headings, and links
pub fn parse_markdown(content: &str) -> Result<ParsedDocument> {
    // Parse frontmatter
    let matter = Matter::<YAML>::new();
    let parsed = matter.parse(content);

    let frontmatter = if let Some(data) = parsed.data {
        // Convert gray_matter's Value to serde_json::Value
        serde_json::from_str(&serde_json::to_string(
            &data.deserialize::<serde_json::Value>()?,
        )?)
        .context("Failed to convert frontmatter to JSON")?
    } else {
        HashMap::new()
    };

    let content_without_frontmatter = parsed.content;

    // Extract wiki-links and tags from the content
    let wiki_links = extract_wiki_links(&content_without_frontmatter)?;
    let tags = extract_tags(&content_without_frontmatter);

    // Parse headings using mq-markdown
    let headings = parse_headings(&content_without_frontmatter)?;

    Ok(ParsedDocument {
        frontmatter,
        content: content_without_frontmatter,
        headings,
        wiki_links,
        tags,
    })
}

/// Extract wiki-style links from content
pub fn extract_wiki_links(content: &str) -> Result<Vec<WikiLink>> {
    let re = Regex::new(r"\[\[([^\]|]+)(?:\|([^\]]+))?\]\]")
        .context("Failed to compile wiki-link regex")?;

    let mut links = Vec::new();
    for cap in re.captures_iter(content) {
        let target = cap.get(1).unwrap().as_str().trim().to_string();
        let alias = cap.get(2).map(|m| m.as_str().trim().to_string());

        links.push(WikiLink { target, alias });
    }

    Ok(links)
}

/// Extract hashtags from content
pub fn extract_tags(content: &str) -> Vec<String> {
    let re = Regex::new(r"(?:^|\s)#([a-zA-Z0-9_/-]+)").unwrap();

    re.captures_iter(content)
        .map(|cap| cap.get(1).unwrap().as_str().to_string())
        .collect()
}

/// Parse markdown headings and their content using mq-lang
fn parse_headings(content: &str) -> Result<Vec<ParsedHeading>> {
    // Use mq_lang to parse markdown and query for all nodes
    let runtime_values = mq_lang::parse_markdown_input(content)
        .map_err(|e| anyhow::anyhow!("Failed to parse markdown with mq-lang: {}", e))?;

    let mut engine = mq_lang::DefaultEngine::default();
    let input = runtime_values.into_iter();

    // Use mq query to get all nodes, then filter for headings in Rust
    let all_nodes = engine
        .eval("nodes", input)
        .map_err(|e| anyhow::anyhow!("Failed to query nodes: {}", e))?;

    let mut heading_matches: Vec<(usize, u8, String, usize)> = Vec::new();

    // Process all nodes and filter for heading nodes
    for value in all_nodes.values() {
        if let mq_lang::RuntimeValue::Markdown(node, _) = value {
            let (level, title) = extract_heading_info(node);
            if level > 0 {
                // Find this heading in the content to get its position
                // We need positions to extract content between headings
                if let Some((start, end)) = find_heading_position(content, level, &title) {
                    heading_matches.push((start, level, title, end));
                }
            }
        }
    }

    // Sort by position in document
    heading_matches.sort_by_key(|m| m.0);

    let mut headings = Vec::new();

    // Extract content for each heading (content between this heading and the next)
    for i in 0..heading_matches.len() {
        let (start, level, title, heading_end) = &heading_matches[i];

        // Content starts after the heading line
        let content_start = *heading_end;

        // Content ends at the start of the next heading (or end of document)
        let content_end = if i + 1 < heading_matches.len() {
            heading_matches[i + 1].0
        } else {
            content.len()
        };

        let heading_content = content[content_start..content_end].trim().to_string();

        headings.push(ParsedHeading {
            level: *level,
            title: title.clone(),
            content: heading_content,
            start_offset: *start,
            end_offset: content_end,
        });
    }

    Ok(headings)
}

/// Extract heading level and title from an mq-lang markdown node
fn extract_heading_info(node: &impl std::fmt::Debug) -> (u8, String) {
    // Use debug string to extract heading info
    // This is necessary because mq-lang doesn't expose the node structure directly
    let debug_str = format!("{:?}", node);

    // Look for heading patterns in the debug output
    // Format is typically: Heading { depth: N, ... children: [Text { value: "Title" }] }
    if debug_str.contains("Heading") {
        let level = extract_level_from_debug(&debug_str).unwrap_or(0);
        let title = extract_title_from_debug(&debug_str);
        return (level, title);
    }

    (0, String::new())
}

fn extract_level_from_debug(debug_str: &str) -> Option<u8> {
    // Look for "depth: X" pattern in debug output
    if let Some(pos) = debug_str.find("depth: ") {
        let rest = &debug_str[pos + 7..];
        if let Some(end) = rest.find([',', ' ', '}']) {
            let level_str = &rest[..end];
            return level_str.parse().ok();
        }
    }
    None
}

fn extract_title_from_debug(debug_str: &str) -> String {
    // Extract text content from heading children
    // Look for Text { value: "..." } patterns
    if let Some(start) = debug_str.find("value: \"") {
        let rest = &debug_str[start + 8..];
        if let Some(end) = rest.find('"') {
            return rest[..end].to_string();
        }
    }
    "Unknown Heading".to_string()
}

/// Find the position of a heading in the content
fn find_heading_position(content: &str, level: u8, title: &str) -> Option<(usize, usize)> {
    let heading_prefix = "#".repeat(level as usize);
    let heading_line = format!("{} {}", heading_prefix, title);

    // Find the heading in the content
    if let Some(start) = content.find(&heading_line) {
        // Find the end of the line
        let end = content[start..]
            .find('\n')
            .map(|offset| start + offset)
            .unwrap_or(content.len());
        return Some((start, end));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_wiki_links() {
        let content = "Here is a [[simple link]] and [[link with|alias]].";
        let links = extract_wiki_links(content).unwrap();

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "simple link");
        assert_eq!(links[0].alias, None);
        assert_eq!(links[1].target, "link with");
        assert_eq!(links[1].alias, Some("alias".to_string()));
    }

    #[test]
    fn test_extract_tags() {
        let content = "Some text #tag1 and #tag2/subtag but not#invalid";
        let tags = extract_tags(content);

        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0], "tag1");
        assert_eq!(tags[1], "tag2/subtag");
    }

    #[test]
    fn test_parse_markdown_with_headings() {
        let content = r#"---
title: "Test Document"
---

# Heading 1

Some content under heading 1.

## Heading 2

More content here with a [[wiki link]].

### Heading 3

Final content with #tag1 and #tag2.
"#;

        let parsed = parse_markdown(content).unwrap();

        println!("✅ Parse successful!");
        println!("Frontmatter entries: {}", parsed.frontmatter.len());
        println!("Headings: {}", parsed.headings.len());
        for (i, heading) in parsed.headings.iter().enumerate() {
            println!(
                "  Heading {}: level={}, title=\"{}\"",
                i + 1,
                heading.level,
                heading.title
            );
        }
        println!("Wiki links: {}", parsed.wiki_links.len());
        for (i, link) in parsed.wiki_links.iter().enumerate() {
            println!(
                "  Link {}: target=\"{}\" alias={:?}",
                i + 1,
                link.target,
                link.alias
            );
        }
        println!("Tags: {}", parsed.tags.len());
        for (i, tag) in parsed.tags.iter().enumerate() {
            println!("  Tag {}: \"{}\"", i + 1, tag);
        }

        assert_eq!(parsed.frontmatter.len(), 1);
        assert_eq!(parsed.headings.len(), 3);
        assert_eq!(parsed.headings[0].title, "Heading 1");
        assert_eq!(parsed.headings[0].level, 1);
        assert_eq!(parsed.headings[1].title, "Heading 2");
        assert_eq!(parsed.headings[1].level, 2);
        assert_eq!(parsed.headings[2].title, "Heading 3");
        assert_eq!(parsed.headings[2].level, 3);

        assert_eq!(parsed.wiki_links.len(), 1);
        assert_eq!(parsed.wiki_links[0].target, "wiki link");

        assert_eq!(parsed.tags.len(), 2);
        assert!(parsed.tags.contains(&"tag1".to_string()));
        assert!(parsed.tags.contains(&"tag2".to_string()));
    }
}
