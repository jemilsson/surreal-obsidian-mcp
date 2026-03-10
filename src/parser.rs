use anyhow::{Context, Result};
use gray_matter::Matter;
use gray_matter::engine::YAML;
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
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
        serde_json::from_str(&serde_json::to_string(&data.deserialize::<serde_json::Value>()?)?)
            .context("Failed to convert frontmatter to JSON")?
    } else {
        HashMap::new()
    };

    let content_without_frontmatter = parsed.content;

    // Extract wiki-links
    let wiki_links = extract_wiki_links(&content_without_frontmatter)?;

    // Extract tags
    let tags = extract_tags(&content_without_frontmatter);

    // Parse headings
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

/// Parse markdown headings and their content
fn parse_headings(content: &str) -> Result<Vec<ParsedHeading>> {
    let parser = Parser::new(content);
    let mut headings = Vec::new();
    let mut current_heading: Option<(HeadingLevel, String, usize)> = None;
    let mut current_text = String::new();
    let mut in_heading = false;

    // We need to track byte offsets
    let mut char_offset = 0;

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                // Save previous heading if exists
                if let Some((prev_level, title, start)) = current_heading.take() {
                    headings.push(ParsedHeading {
                        level: heading_level_to_u8(prev_level),
                        title: title.clone(),
                        content: current_text.trim().to_string(),
                        start_offset: start,
                        end_offset: char_offset,
                    });
                    current_text.clear();
                }

                in_heading = true;
                current_heading = Some((level, String::new(), char_offset));
            }
            Event::End(TagEnd::Heading(_)) => {
                in_heading = false;
            }
            Event::Text(text) => {
                if in_heading {
                    if let Some((_, ref mut title, _)) = current_heading {
                        title.push_str(&text);
                    }
                } else {
                    current_text.push_str(&text);
                }
                char_offset += text.len();
            }
            Event::Code(text) | Event::Html(text) => {
                if !in_heading {
                    current_text.push_str(&text);
                }
                char_offset += text.len();
            }
            Event::SoftBreak | Event::HardBreak => {
                if !in_heading {
                    current_text.push('\n');
                }
                char_offset += 1;
            }
            _ => {}
        }
    }

    // Save last heading if exists
    if let Some((level, title, start)) = current_heading {
        headings.push(ParsedHeading {
            level: heading_level_to_u8(level),
            title,
            content: current_text.trim().to_string(),
            start_offset: start,
            end_offset: char_offset,
        });
    }

    Ok(headings)
}

/// Convert HeadingLevel to u8
fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
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
        assert!(tags.contains(&"tag1".to_string()));
        assert!(tags.contains(&"tag2/subtag".to_string()));
    }

    #[test]
    fn test_parse_markdown_with_frontmatter() {
        let markdown = r#"---
title: Test Note
tags:
  - test
  - example
---

# Heading 1

Some content with [[wiki link]].

## Heading 2

More content with #tag.
"#;

        let parsed = parse_markdown(markdown).unwrap();

        assert!(parsed.frontmatter.contains_key("title"));
        assert_eq!(parsed.headings.len(), 2);
        assert_eq!(parsed.headings[0].level, 1);
        assert_eq!(parsed.headings[0].title, "Heading 1");
        assert_eq!(parsed.wiki_links.len(), 1);
        assert_eq!(parsed.wiki_links[0].target, "wiki link");
        assert!(parsed.tags.contains(&"tag".to_string()));
    }

    #[test]
    fn test_parse_markdown_without_frontmatter() {
        let markdown = r#"# My Note

This is content with [[another note]].

## Section

Content here.
"#;

        let parsed = parse_markdown(markdown).unwrap();

        assert!(parsed.frontmatter.is_empty());
        assert_eq!(parsed.headings.len(), 2);
        assert_eq!(parsed.headings[0].level, 1);
        assert_eq!(parsed.headings[1].level, 2);
        assert_eq!(parsed.wiki_links.len(), 1);
    }

    #[test]
    fn test_nested_headings() {
        let markdown = r#"# H1

Content for H1.

## H2

Content for H2.

### H3

Content for H3.
"#;

        let parsed = parse_markdown(markdown).unwrap();

        assert_eq!(parsed.headings.len(), 3);
        assert_eq!(parsed.headings[0].level, 1);
        assert_eq!(parsed.headings[1].level, 2);
        assert_eq!(parsed.headings[2].level, 3);
    }
}
