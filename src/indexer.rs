use anyhow::{Context, Result};
use std::path::Path;

use crate::db::Block;
use crate::parser::{parse_markdown, ParsedDocument};

/// Extract blocks from a markdown file
/// Returns a vector of blocks: first is the file (level 0), then headings (level 1-6)
pub fn extract_blocks_from_file<P: AsRef<Path>>(
    file_path: P,
    vault_root: P,
) -> Result<Vec<Block>> {
    let absolute_path = file_path.as_ref();
    let vault_root = vault_root.as_ref();

    // Read file content
    let content = std::fs::read_to_string(absolute_path)
        .with_context(|| format!("Failed to read file: {}", absolute_path.display()))?;

    // Get relative path from vault root
    let relative_path = absolute_path
        .strip_prefix(vault_root)
        .context("File path is not within vault root")?
        .to_string_lossy()
        .to_string();

    // Parse markdown
    let parsed = parse_markdown(&content)?;

    // Extract blocks
    extract_blocks_from_parsed(&relative_path, &content, &parsed)
}

/// Extract blocks from parsed markdown document
pub fn extract_blocks_from_parsed(
    file_path: &str,
    _full_content: &str,
    parsed: &ParsedDocument,
) -> Result<Vec<Block>> {
    let mut blocks = Vec::new();

    // Create file block (level 0)
    let file_title = extract_file_title(file_path, parsed);
    let file_content = if parsed.headings.is_empty() {
        // No headings, use all content
        parsed.content.clone()
    } else {
        // Content before first heading
        parsed.content[..parsed.headings[0].start_offset]
            .trim()
            .to_string()
    };

    // Convert frontmatter to HashMap<String, String>
    let properties = convert_frontmatter_to_string_map(&parsed.frontmatter);

    // Generate ID upfront so parent-child relationships can reference it
    let file_id = format!("block_{}", uuid::Uuid::new_v4().simple());

    let file_block = Block {
        id: file_id,
        level: 0,
        title: file_title,
        content: file_content,
        file_path: file_path.to_string(),
        parent_id: None,
        children_ids: Vec::new(),
        properties,
        tags: parsed.tags.clone(),
        position: 0, // File block is always first
        created_at: 0, // Will be set on insert
        updated_at: 0, // Will be set on insert
        embedding: None,
        content_hash: None,
        outgoing_links: parsed
            .wiki_links
            .iter()
            .map(|link| link.target.clone())
            .collect(),
        incoming_links: Vec::new(), // Will be populated during indexing
    };

    blocks.push(file_block);

    // Create blocks for each heading
    for (index, heading) in parsed.headings.iter().enumerate() {
        // Determine parent: either the file block or a previous heading with lower level
        let parent_id = find_parent_for_heading(&blocks, heading.level);

        // Generate ID upfront so parent-child relationships can reference it
        let heading_id = format!("block_{}", uuid::Uuid::new_v4().simple());

        let heading_block = Block {
            id: heading_id,
            level: heading.level,
            title: heading.title.clone(),
            content: heading.content.clone(),
            file_path: file_path.to_string(),
            parent_id: Some(parent_id),
            children_ids: Vec::new(),
            properties: std::collections::BTreeMap::new(),
            tags: extract_tags_from_content(&heading.content),
            position: (index + 1) as i32, // Position in document (file is 0, headings are 1+)
            created_at: 0,
            updated_at: 0,
            embedding: None,
            content_hash: None,
            outgoing_links: extract_links_from_content(&heading.content),
            incoming_links: Vec::new(),
        };

        blocks.push(heading_block);
    }

    // Build children_ids relationships
    build_children_relationships(&mut blocks);

    Ok(blocks)
}

/// Convert frontmatter HashMap<String, serde_json::Value> to BTreeMap<String, String>
fn convert_frontmatter_to_string_map(
    frontmatter: &std::collections::HashMap<String, serde_json::Value>,
) -> std::collections::BTreeMap<String, String> {
    frontmatter
        .iter()
        .map(|(k, v)| {
            let value_str = match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Null => "null".to_string(),
                _ => serde_json::to_string(v).unwrap_or_else(|_| "".to_string()),
            };
            (k.clone(), value_str)
        })
        .collect()
}

/// Extract file title from file path or frontmatter
fn extract_file_title(file_path: &str, parsed: &ParsedDocument) -> String {
    // Try to get title from frontmatter
    if let Some(title_value) = parsed.frontmatter.get("title") {
        if let Some(title_str) = title_value.as_str() {
            return title_str.to_string();
        }
    }

    // Fall back to filename without extension
    Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

/// Find the parent block ID for a heading based on level
fn find_parent_for_heading(blocks: &[Block], level: u8) -> String {
    // Find the most recent block with a lower level
    for block in blocks.iter().rev() {
        if block.level < level {
            return block.id.clone();
        }
    }

    // If no parent found, the file block (first in the list) is the parent
    blocks[0].id.clone()
}

/// Extract wiki-links from content
fn extract_links_from_content(content: &str) -> Vec<String> {
    use crate::parser::extract_wiki_links;

    match extract_wiki_links(content) {
        Ok(links) => links.iter().map(|link| link.target.clone()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Extract tags from content
fn extract_tags_from_content(content: &str) -> Vec<String> {
    use crate::parser::extract_tags;

    extract_tags(content)
}

/// Build parent-child relationships by populating children_ids
fn build_children_relationships(blocks: &mut [Block]) {
    use std::collections::HashMap;

    // Build a map of parent_id -> list of child IDs (not indices)
    let mut parent_children: HashMap<String, Vec<String>> = HashMap::new();

    for block in blocks.iter() {
        if let Some(parent_id) = &block.parent_id {
            parent_children
                .entry(parent_id.clone())
                .or_insert_with(Vec::new)
                .push(block.id.clone());
        }
    }

    // Update children_ids for each block
    for block in blocks.iter_mut() {
        if let Some(child_ids) = parent_children.get(&block.id) {
            block.children_ids = child_ids.clone();
        }
    }
}

/// Make functions public for parser module
pub use crate::parser::{extract_tags, extract_wiki_links};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_extract_blocks_simple() {
        let markdown = r#"# My Note

This is some content.

## Section 1

Content for section 1.

## Section 2

Content for section 2.
"#;

        let parsed = parse_markdown(markdown).unwrap();
        let blocks = extract_blocks_from_parsed("test.md", markdown, &parsed).unwrap();

        // Should have 1 file block + 3 heading blocks
        assert_eq!(blocks.len(), 4);

        // File block
        assert_eq!(blocks[0].level, 0);
        assert_eq!(blocks[0].title, "test");
        assert_eq!(blocks[0].parent_id, None);

        // First heading (H1)
        assert_eq!(blocks[1].level, 1);
        assert_eq!(blocks[1].title, "My Note");

        // Second heading (H2)
        assert_eq!(blocks[2].level, 2);
        assert_eq!(blocks[2].title, "Section 1");

        // Third heading (H2)
        assert_eq!(blocks[3].level, 2);
        assert_eq!(blocks[3].title, "Section 2");
    }

    #[test]
    fn test_extract_blocks_with_frontmatter() {
        let markdown = r#"---
title: Custom Title
tags:
  - tag1
  - tag2
---

# Heading

Content here.
"#;

        let parsed = parse_markdown(markdown).unwrap();
        let blocks = extract_blocks_from_parsed("test.md", markdown, &parsed).unwrap();

        // File block should have frontmatter
        assert_eq!(blocks[0].title, "Custom Title");
        assert!(blocks[0].properties.contains_key("title"));
        assert!(blocks[0].properties.contains_key("tags"));
    }

    #[test]
    fn test_extract_blocks_with_links() {
        let markdown = r#"# Note

This links to [[other note]] and [[another|alias]].

## Section

More links: [[third note]].
"#;

        let parsed = parse_markdown(markdown).unwrap();
        let blocks = extract_blocks_from_parsed("test.md", markdown, &parsed).unwrap();

        // File block should have links from entire document
        assert_eq!(blocks[0].outgoing_links.len(), 3);

        // Heading should have links from its content
        assert_eq!(blocks[2].outgoing_links.len(), 1);
        assert_eq!(blocks[2].outgoing_links[0], "third note");
    }

    #[test]
    fn test_extract_blocks_hierarchy() {
        let markdown = r#"# H1

Content for H1.

## H2

Content for H2.

### H3

Content for H3.

## Another H2

Content.
"#;

        let parsed = parse_markdown(markdown).unwrap();
        let blocks = extract_blocks_from_parsed("test.md", markdown, &parsed).unwrap();

        assert_eq!(blocks.len(), 5); // 1 file + 4 headings

        // H1 should be child of file
        assert_eq!(blocks[1].level, 1);
        assert_eq!(blocks[1].parent_id, Some(blocks[0].id.clone()));

        // First H2 should be child of H1
        assert_eq!(blocks[2].level, 2);
        assert_eq!(blocks[2].parent_id, Some(blocks[1].id.clone()));

        // H3 should be child of H2
        assert_eq!(blocks[3].level, 3);
        assert_eq!(blocks[3].parent_id, Some(blocks[2].id.clone()));

        // Second H2 should be child of H1
        assert_eq!(blocks[4].level, 2);
        assert_eq!(blocks[4].parent_id, Some(blocks[1].id.clone()));
    }

    #[test]
    fn test_extract_from_file() {
        let dir = tempdir().unwrap();
        let vault_path = dir.path().to_path_buf();
        let file_path = vault_path.join("test.md");

        let content = r#"---
title: Test File
---

# My Heading

Content with [[link]].
"#;

        fs::write(&file_path, content).unwrap();

        let blocks = extract_blocks_from_file(&file_path, &vault_path).unwrap();

        assert_eq!(blocks.len(), 2); // File + 1 heading
        assert_eq!(blocks[0].file_path, "test.md");
        assert_eq!(blocks[0].title, "Test File");
    }
}
