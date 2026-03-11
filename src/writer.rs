use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

use crate::db::Block;

/// Write a new markdown file from a block
pub fn write_new_file<P: AsRef<Path>>(
    vault_root: P,
    file_path: &str,
    block: &Block,
) -> Result<PathBuf> {
    let vault_root = vault_root.as_ref();
    let absolute_path = vault_root.join(file_path);

    // Ensure parent directory exists
    if let Some(parent) = absolute_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    // Build markdown content
    let markdown = build_markdown_from_block(block)?;

    // Write atomically: write to temp file, then rename
    let temp_path = absolute_path.with_extension("tmp");
    std::fs::write(&temp_path, &markdown)
        .with_context(|| format!("Failed to write temp file: {}", temp_path.display()))?;

    std::fs::rename(&temp_path, &absolute_path)
        .with_context(|| format!("Failed to rename temp file to: {}", absolute_path.display()))?;

    info!("✅ Created new file: {}", file_path);

    Ok(absolute_path)
}

/// Reconstruct and write a markdown file from all its blocks
pub fn write_file_from_blocks<P: AsRef<Path>>(
    vault_root: P,
    file_path: &str,
    blocks: &[Block],
) -> Result<PathBuf> {
    let vault_root = vault_root.as_ref();
    let absolute_path = vault_root.join(file_path);

    if blocks.is_empty() {
        anyhow::bail!("Cannot write file from empty blocks");
    }

    // Ensure parent directory exists
    if let Some(parent) = absolute_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    // Build markdown content
    let markdown = reconstruct_markdown_from_blocks(blocks)?;

    // Write atomically: write to temp file, then rename
    let temp_path = absolute_path.with_extension("tmp");
    std::fs::write(&temp_path, &markdown)
        .with_context(|| format!("Failed to write temp file: {}", temp_path.display()))?;

    std::fs::rename(&temp_path, &absolute_path)
        .with_context(|| format!("Failed to rename temp file to: {}", absolute_path.display()))?;

    debug!("Wrote file: {}", file_path);

    Ok(absolute_path)
}

/// Delete a markdown file
pub fn delete_file<P: AsRef<Path>>(vault_root: P, file_path: &str) -> Result<()> {
    let vault_root = vault_root.as_ref();
    let absolute_path = vault_root.join(file_path);

    if !absolute_path.exists() {
        anyhow::bail!("File does not exist: {}", file_path);
    }

    std::fs::remove_file(&absolute_path)
        .with_context(|| format!("Failed to delete file: {}", absolute_path.display()))?;

    info!("Deleted file: {}", file_path);

    Ok(())
}

/// Build markdown content from a single file block (level 0)
fn build_markdown_from_block(block: &Block) -> Result<String> {
    if block.level != 0 {
        anyhow::bail!("Can only build file from level 0 block");
    }

    let mut markdown = String::new();

    // Add frontmatter if present
    if !block.properties.is_empty() {
        // Convert HashMap<String, String> to HashMap<String, serde_json::Value> for YAML
        let properties: std::collections::HashMap<String, serde_json::Value> = block
            .properties
            .iter()
            .map(|(k, v)| {
                // Try to parse as JSON value, otherwise use as string
                let value = serde_json::from_str(v).unwrap_or(serde_json::Value::String(v.clone()));
                (k.clone(), value)
            })
            .collect();

        markdown.push_str("---\n");
        let yaml = serde_yaml::to_string(&properties)
            .context("Failed to serialize frontmatter to YAML")?;
        markdown.push_str(&yaml);
        markdown.push_str("---\n\n");
    }

    // Add content
    markdown.push_str(&block.content);

    if !markdown.ends_with('\n') {
        markdown.push('\n');
    }

    Ok(markdown)
}

/// Reconstruct full markdown from all blocks in a file
fn reconstruct_markdown_from_blocks(blocks: &[Block]) -> Result<String> {
    if blocks.is_empty() {
        return Ok(String::new());
    }

    // Sort blocks by position to maintain document order
    let mut sorted_blocks = blocks.to_vec();
    sorted_blocks.sort_by_key(|b| b.position);

    let file_block = sorted_blocks
        .iter()
        .find(|b| b.level == 0)
        .context("No file block (level 0) found")?;

    let mut markdown = String::new();

    // Add frontmatter if present
    if !file_block.properties.is_empty() {
        // Convert HashMap<String, String> to HashMap<String, serde_json::Value> for YAML
        let properties: std::collections::HashMap<String, serde_json::Value> = file_block
            .properties
            .iter()
            .map(|(k, v)| {
                // Try to parse as JSON value, otherwise use as string
                let value = serde_json::from_str(v).unwrap_or(serde_json::Value::String(v.clone()));
                (k.clone(), value)
            })
            .collect();

        markdown.push_str("---\n");
        let yaml = serde_yaml::to_string(&properties)
            .context("Failed to serialize frontmatter to YAML")?;
        markdown.push_str(&yaml);
        markdown.push_str("---\n\n");
    }

    // Add file content (content before first heading)
    if !file_block.content.is_empty() {
        markdown.push_str(&file_block.content);
        if !markdown.ends_with('\n') {
            markdown.push('\n');
        }
        markdown.push('\n');
    }

    // Add headings and their content
    let headings: Vec<&Block> = sorted_blocks.iter().filter(|b| b.level > 0).collect();

    for heading in headings {
        // Add heading marker
        let level_marker = "#".repeat(heading.level as usize);
        markdown.push_str(&format!("{} {}\n\n", level_marker, heading.title));

        // Add heading content
        if !heading.content.is_empty() {
            markdown.push_str(&heading.content);
            if !markdown.ends_with('\n') {
                markdown.push('\n');
            }
            markdown.push('\n');
        }
    }

    Ok(markdown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_build_markdown_from_block() {
        let mut properties = std::collections::BTreeMap::new();
        properties.insert("title".to_string(), "Test".to_string());

        let block = Block {
            id: "test".to_string(),
            level: 0,
            title: "Test Note".to_string(),
            content: "This is content.".to_string(),
            file_path: "test.md".to_string(),
            parent_id: None,
            children_ids: Vec::new(),
            properties,
            tags: Vec::new(),
            position: 0,
            created_at: 0,
            updated_at: 0,
            embedding: None,
            content_hash: None,
            outgoing_links: Vec::new(),
            incoming_links: Vec::new(),
        };

        let markdown = build_markdown_from_block(&block).unwrap();

        assert!(markdown.contains("---"));
        assert!(markdown.contains("title:"));
        assert!(markdown.contains("This is content."));
    }

    #[test]
    fn test_reconstruct_markdown_from_blocks() {
        let file_block = Block {
            id: "file".to_string(),
            level: 0,
            title: "Test".to_string(),
            content: "Intro content.".to_string(),
            file_path: "test.md".to_string(),
            parent_id: None,
            children_ids: vec!["h1".to_string()],
            properties: std::collections::BTreeMap::new(),
            tags: Vec::new(),
            position: 0,
            created_at: 0,
            updated_at: 0,
            embedding: None,
            content_hash: None,
            outgoing_links: Vec::new(),
            incoming_links: Vec::new(),
        };

        let heading_block = Block {
            id: "h1".to_string(),
            level: 1,
            title: "Section".to_string(),
            content: "Section content.".to_string(),
            file_path: "test.md".to_string(),
            parent_id: Some("file".to_string()),
            children_ids: Vec::new(),
            properties: std::collections::BTreeMap::new(),
            tags: Vec::new(),
            position: 1,
            created_at: 1,
            updated_at: 1,
            embedding: None,
            content_hash: None,
            outgoing_links: Vec::new(),
            incoming_links: Vec::new(),
        };

        let blocks = vec![file_block, heading_block];
        let markdown = reconstruct_markdown_from_blocks(&blocks).unwrap();

        assert!(markdown.contains("Intro content."));
        assert!(markdown.contains("# Section"));
        assert!(markdown.contains("Section content."));
    }

    #[test]
    fn test_write_new_file() {
        let dir = tempdir().unwrap();
        let vault_path = dir.path();

        let block = Block {
            id: "test".to_string(),
            level: 0,
            title: "New Note".to_string(),
            content: "This is a new note.".to_string(),
            file_path: "new.md".to_string(),
            parent_id: None,
            children_ids: Vec::new(),
            properties: std::collections::BTreeMap::new(),
            tags: Vec::new(),
            position: 0,
            created_at: 0,
            updated_at: 0,
            embedding: None,
            content_hash: None,
            outgoing_links: Vec::new(),
            incoming_links: Vec::new(),
        };

        let result = write_new_file(vault_path, "new.md", &block);
        assert!(result.is_ok());

        let written_path = vault_path.join("new.md");
        assert!(written_path.exists());

        let content = std::fs::read_to_string(written_path).unwrap();
        assert!(content.contains("This is a new note."));
    }
}
