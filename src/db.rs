use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
#[cfg(feature = "persistent")]
use surrealdb::engine::local::{Db, SurrealKv};
#[cfg(not(feature = "persistent"))]
use surrealdb::engine::local::{Db, Mem};
use surrealdb::{Surreal, RecordId};
use tracing::info;
use uuid::Uuid;

/// Internal database record with RecordId for SurrealDB 3.0 compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BlockRecord {
    id: RecordId,
    level: u8,
    title: String,
    content: String,
    file_path: String,
    content_address: String,
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    children_ids: Vec<String>,
    #[serde(default)]
    properties: BTreeMap<String, String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    position: i32,
    created_at: i64,
    updated_at: i64,
    #[serde(default)]
    embedding: Option<Vec<f32>>,
    #[serde(default)]
    content_hash: Option<String>,
    #[serde(default)]
    outgoing_links: Vec<String>,
    #[serde(default)]
    incoming_links: Vec<String>,
}

impl BlockRecord {
    /// Convert BlockRecord to Block by extracting the ID string
    fn to_block(self) -> Block {
        // The RecordId.key can be a String, Number, or other types
        // We need to extract just the value without the Debug wrapper
        let key_string = format!("{:?}", self.id.key());
        // Remove "String(" prefix and ")" suffix if present, then trim quotes
        let id = if key_string.starts_with("String(\"") && key_string.ends_with("\")") {
            key_string[8..key_string.len() - 2].to_string()
        } else {
            key_string.trim_matches('"').to_string()
        };

        Block {
            id,
            level: self.level,
            title: self.title,
            content: self.content,
            file_path: self.file_path,
            content_address: self.content_address,
            parent_id: self.parent_id,
            children_ids: self.children_ids,
            properties: self.properties,
            tags: self.tags,
            position: self.position,
            created_at: self.created_at,
            updated_at: self.updated_at,
            embedding: self.embedding,
            content_hash: self.content_hash,
            outgoing_links: self.outgoing_links,
            incoming_links: self.incoming_links,
        }
    }

    /// Convert a vector of BlockRecords to Blocks
    fn to_blocks(records: Vec<BlockRecord>) -> Vec<Block> {
        records.into_iter().map(|r| r.to_block()).collect()
    }
}

/// Block represents a piece of content in the vault
/// Level 0 = file/document, Level 1-6 = headings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub id: String,
    pub level: u8, // 0 for files, 1-6 for headings
    pub title: String,
    pub content: String,
    pub file_path: String, // Relative path from vault root

    // Content addressing (human-readable reference)
    pub content_address: String, // e.g., "project.md" or "project.md#Overview"

    // Hierarchy
    pub parent_id: Option<String>,
    pub children_ids: Vec<String>,

    // Metadata
    pub properties: BTreeMap<String, String>, // Frontmatter for level 0 (string values only)
    pub tags: Vec<String>,
    #[serde(default)]
    pub position: i32, // Position in document (for correct ordering when reconstructing)
    pub created_at: i64, // Unix timestamp in seconds
    pub updated_at: i64, // Unix timestamp in seconds

    // Embeddings (optional, added during indexing)
    #[serde(default)]
    pub embedding: Option<Vec<f32>>,
    #[serde(default)]
    pub content_hash: Option<String>, // SHA256 hash of title+content for embedding cache

    // Links
    pub outgoing_links: Vec<String>, // Block IDs this block links to
    pub incoming_links: Vec<String>, // Block IDs that link to this block
}

/// Database connection and operations
pub struct Database {
    db: Surreal<Db>,
}

impl Database {
    /// Create a new database connection
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        info!("Initializing SurrealDB at {}", path.as_ref().display());

        #[cfg(feature = "persistent")]
        let db = Surreal::new::<SurrealKv>(path.as_ref())
            .await
            .context("Failed to create SurrealDB instance")?;
        
        #[cfg(not(feature = "persistent"))]
        let db = Surreal::new::<Mem>(())
            .await
            .context("Failed to create SurrealDB instance")?;

        // Use namespace and database
        db.use_ns("obsidian")
            .use_db("vault")
            .await
            .context("Failed to use namespace/database")?;

        let database = Self { db };

        // Initialize schema
        database.init_schema().await?;

        info!("✅ SurrealDB initialized");

        Ok(database)
    }

    /// Initialize the database schema
    async fn init_schema(&self) -> Result<()> {
        // Define block table with indexes
        self.db
            .query(
                "
                DEFINE TABLE IF NOT EXISTS blocks SCHEMALESS;

                DEFINE INDEX IF NOT EXISTS idx_file_path ON blocks FIELDS file_path;
                DEFINE INDEX IF NOT EXISTS idx_level ON blocks FIELDS level;
                DEFINE INDEX IF NOT EXISTS idx_parent ON blocks FIELDS parent_id;
                ",
            )
            .await
            .context("Failed to create schema")?;

        Ok(())
    }

    /// Create a new block
    pub async fn create_block(&self, mut block: Block) -> Result<Block> {
        // Generate ID if not provided
        if block.id.is_empty() {
            block.id = format!("block_{}", Uuid::new_v4().simple());
        }

        // Set timestamps
        let now = chrono::Utc::now().timestamp();
        block.created_at = now;
        block.updated_at = now;

        let block_id = block.id.clone();

        // Convert Block to JSON, excluding the id field and null values
        let mut block_json = serde_json::to_value(&block).context("Failed to serialize block")?;
        if let Some(obj) = block_json.as_object_mut() {
            obj.remove("id");
            // Remove null values to avoid SurrealDB deserialization issues
            obj.retain(|_, v| !v.is_null());
        }

        // Insert the block using create() for SurrealDB 2.6 compatibility
        // We immediately fetch it back with get_block(), so we don't need the result here
        self
            .db
            .query("CREATE type::thing($table, $id) CONTENT $content")
            .bind(("table", "blocks"))
            .bind(("id", block_id.clone()))
            .bind(("content", block_json))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to insert block with ID {}: {:?}", block_id, e))?;

        // Query the block back using our SELECT_BLOCK_FIELDS which converts RecordId to String
        self.get_block(&block_id)
            .await?
            .context("Block not found after creation")
    }

    /// Get a block by ID
    pub async fn get_block(&self, id: &str) -> Result<Option<Block>> {
        // Use BlockRecord for database operations, convert to Block
        let result: Option<BlockRecord> = self
            .db
            .select(("blocks", id))
            .await
            .context("Failed to get block")?;

        Ok(result.map(|record| record.to_block()))
    }

    /// Get a block by content address (e.g., "project.md" or "project.md#Overview")
    pub async fn get_block_by_address(&self, address: &str) -> Result<Option<Block>> {
        let records: Vec<BlockRecord> = self
            .db
            .query("SELECT * FROM blocks WHERE content_address = $address LIMIT 1")
            .bind(("address", address.to_string()))
            .await
            .context("Failed to query block by content address")?
            .take(0)
            .context("Failed to parse query result")?;

        Ok(records.into_iter().next().map(|record| record.to_block()))
    }

    /// Get a block by either ID or content address (tries ID first, then address)
    pub async fn get_block_by_id_or_address(&self, id_or_address: &str) -> Result<Option<Block>> {
        // Try as block ID first
        if let Some(block) = self.get_block(id_or_address).await? {
            return Ok(Some(block));
        }

        // Fall back to content address lookup
        self.get_block_by_address(id_or_address).await
    }

    /// Update a block
    pub async fn update_block(&self, id: &str, mut block: Block) -> Result<Block> {
        block.updated_at = chrono::Utc::now().timestamp();

        // Convert Block to JSON, excluding the id field and null values
        let mut block_json = serde_json::to_value(&block).context("Failed to serialize block")?;
        if let Some(obj) = block_json.as_object_mut() {
            obj.remove("id");
            // Remove null values to avoid SurrealDB deserialization issues
            obj.retain(|_, v| !v.is_null());
        }

        // Update using query for SurrealDB 2.6 compatibility
        self
            .db
            .query("UPDATE type::thing($table, $id) CONTENT $content")
            .bind(("table", "blocks"))
            .bind(("id", id.to_string()))
            .bind(("content", block_json))
            .await
            .context("Failed to update block")?;

        // Query the block back using our SELECT_BLOCK_FIELDS which converts RecordId to String
        self.get_block(id)
            .await?
            .context("Block not found after update")
    }

    /// Delete a block
    pub async fn delete_block(&self, id: &str) -> Result<()> {
        self
            .db
            .query("DELETE type::thing($table, $id)")
            .bind(("table", "blocks"))
            .bind(("id", id.to_string()))
            .await
            .context("Failed to delete block")?;

        Ok(())
    }

    /// Get all blocks for a file
    pub async fn get_blocks_by_file(&self, file_path: &str) -> Result<Vec<Block>> {
        let records: Vec<BlockRecord> = self
            .db
            .query("SELECT * FROM blocks WHERE file_path = $path ORDER BY level ASC")
            .bind(("path", file_path.to_string()))
            .await
            .context("Failed to query blocks by file")?
            .take(0)
            .context("Failed to parse query result")?;

        Ok(BlockRecord::to_blocks(records))
    }

    /// Get children of a block
    pub async fn get_children(&self, parent_id: &str) -> Result<Vec<Block>> {
        let records: Vec<BlockRecord> = self
            .db
            .query("SELECT * FROM blocks WHERE parent_id = $parent ORDER BY created_at ASC")
            .bind(("parent", parent_id.to_string()))
            .await
            .context("Failed to query child blocks")?
            .take(0)
            .context("Failed to parse query result")?;

        Ok(BlockRecord::to_blocks(records))
    }

    /// Search blocks by content (simple text search for now)
    pub async fn search_blocks(&self, query: &str, limit: usize) -> Result<Vec<Block>> {
        let records: Vec<BlockRecord> = if query.is_empty() {
            // If query is empty, return all blocks using select() API
            self.db
                .select("blocks")
                .await
                .context("Failed to search blocks")?
        } else {
            // Search with query string
            self.db
                .query(
                    "SELECT * FROM blocks
                     WHERE string::lowercase(title) CONTAINS string::lowercase($query)
                        OR string::lowercase(content) CONTAINS string::lowercase($query)
                     LIMIT $limit",
                )
                .bind(("query", query.to_string()))
                .bind(("limit", limit))
                .await
                .context("Failed to search blocks")?
                .take(0)
                .context("Failed to parse query result")?
        };

        Ok(BlockRecord::to_blocks(records))
    }

    /// Get all root blocks (level 0 - files)
    pub async fn get_all_files(&self) -> Result<Vec<Block>> {
        let records: Vec<BlockRecord> = self
            .db
            .query("SELECT * FROM blocks WHERE level = 0 ORDER BY file_path ASC")
            .await
            .context("Failed to query files")?
            .take(0)
            .context("Failed to parse query result")?;

        Ok(BlockRecord::to_blocks(records))
    }

    /// Search for similar blocks using vector similarity
    /// Returns blocks ordered by similarity (most similar first)
    pub async fn search_similar(&self, embedding: Vec<f32>, limit: usize) -> Result<Vec<Block>> {
        // Use SurrealDB's vector::similarity function to find similar embeddings
        // This performs cosine similarity search
        let records: Vec<BlockRecord> = self
            .db
            .query(
                "SELECT * FROM blocks
                 WHERE embedding IS NOT NONE
                 ORDER BY vector::similarity::cosine(embedding, $query_embedding) DESC
                 LIMIT $limit",
            )
            .bind(("query_embedding", embedding))
            .bind(("limit", limit))
            .await
            .context("Failed to search similar blocks")?
            .take(0)
            .context("Failed to parse query result")?;

        Ok(BlockRecord::to_blocks(records))
    }

    /// Get blocks that need embeddings (blocks without embeddings)
    pub async fn get_blocks_without_embeddings(&self, limit: usize) -> Result<Vec<Block>> {
        let records: Vec<BlockRecord> = self
            .db
            .query(
                "SELECT * FROM blocks
                 WHERE embedding IS NONE
                 LIMIT $limit",
            )
            .bind(("limit", limit))
            .await
            .context("Failed to query blocks without embeddings")?
            .take(0)
            .context("Failed to parse query result")?;

        Ok(BlockRecord::to_blocks(records))
    }

    /// Get blocks that this block links to (outgoing links)
    pub async fn get_linked_blocks(&self, block_id: &str) -> Result<Vec<Block>> {
        // Get the source block
        let block = self.get_block(block_id).await?.context("Block not found")?;

        if block.outgoing_links.is_empty() {
            return Ok(Vec::new());
        }

        // Find blocks where the title or file_path matches any of the link targets
        let mut linked_blocks = Vec::new();
        for link_target in &block.outgoing_links {
            let records: Vec<BlockRecord> = self
                .db
                .query(
                    "SELECT * FROM blocks
                     WHERE title = $target OR file_path = $target",
                )
                .bind(("target", link_target.clone()))
                .await
                .context("Failed to query linked blocks")?
                .take(0)
                .context("Failed to parse query result")?;

            linked_blocks.extend(BlockRecord::to_blocks(records));
        }

        Ok(linked_blocks)
    }

    /// Get blocks that link to this block (incoming links/backlinks)
    pub async fn get_backlinks(&self, block_id: &str) -> Result<Vec<Block>> {
        // Get the target block
        let block = self.get_block(block_id).await?.context("Block not found")?;

        if block.incoming_links.is_empty() {
            return Ok(Vec::new());
        }

        // Get blocks by their IDs
        let mut backlink_blocks = Vec::new();
        for link_id in &block.incoming_links {
            if let Some(b) = self.get_block(link_id).await? {
                backlink_blocks.push(b);
            }
        }

        Ok(backlink_blocks)
    }

    /// Find blocks by tag
    pub async fn find_by_tag(&self, tag: &str, limit: usize) -> Result<Vec<Block>> {
        let records: Vec<BlockRecord> = self
            .db
            .query(
                "SELECT * FROM blocks
                 WHERE $tag IN tags
                 LIMIT $limit",
            )
            .bind(("tag", tag.to_string()))
            .bind(("limit", limit))
            .await
            .context("Failed to query blocks by tag")?
            .take(0)
            .context("Failed to parse query result")?;

        Ok(BlockRecord::to_blocks(records))
    }

    /// Delete all blocks for a specific file
    pub async fn delete_blocks_by_file(&self, file_path: &str) -> Result<()> {
        let _: Vec<serde_json::Value> = self
            .db
            .query("DELETE FROM blocks WHERE file_path = $path")
            .bind(("path", file_path.to_string()))
            .await
            .context("Failed to delete blocks by file")?
            .take(0)
            .context("Failed to parse delete result")?;

        Ok(())
    }
}

impl Block {
    /// Generate a content address for a block
    /// Format: "file.md" for level 0, "file.md#Heading" for headings
    pub fn generate_content_address(file_path: &str, level: u8, title: &str) -> String {
        if level == 0 {
            // File-level block: just the file path
            file_path.to_string()
        } else {
            // Heading block: file_path#heading_title
            format!("{}#{}", file_path, title)
        }
    }

    /// Create a new block
    pub fn new(level: u8, title: String, content: String, file_path: String) -> Self {
        let content_address = Self::generate_content_address(&file_path, level, &title);

        Self {
            id: String::new(), // Will be generated on insert
            level,
            title,
            content,
            file_path,
            content_address,
            parent_id: None,
            children_ids: Vec::new(),
            properties: BTreeMap::new(),
            tags: Vec::new(),
            position: 0,
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
            embedding: None,
            content_hash: None,
            outgoing_links: Vec::new(),
            incoming_links: Vec::new(),
        }
    }

    /// Check if this is a file/document (level 0)
    pub fn is_file(&self) -> bool {
        self.level == 0
    }

    /// Check if this is a heading
    pub fn is_heading(&self) -> bool {
        self.level > 0 && self.level <= 6
    }

    /// Compute SHA256 hash of title and content for embedding cache
    pub fn compute_content_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.title.as_bytes());
        hasher.update(b"\n");
        hasher.update(self.content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Check if content has changed by comparing hashes
    pub fn content_changed(&self, old_hash: Option<&str>) -> bool {
        match old_hash {
            None => true, // No hash means content is new
            Some(hash) => hash != self.compute_content_hash(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_database_creation() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let _db = Database::new(&db_path).await.unwrap();
        // With kv-mem backend (default), no file is created
        // Only check file existence with persistent feature
        #[cfg(feature = "persistent")]
        assert!(db_path.exists());
    }

    #[tokio::test]
    async fn test_block_crud() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::new(&db_path).await.unwrap();

        // Create
        let block = Block::new(
            0,
            "Test Note".to_string(),
            "Content here".to_string(),
            "test.md".to_string(),
        );
        let created = db.create_block(block).await.unwrap();
        assert!(!created.id.is_empty());
        assert_eq!(created.title, "Test Note");

        // Read
        let retrieved = db.get_block(&created.id).await.unwrap().unwrap();
        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.title, "Test Note");

        // Update
        let mut updated = retrieved.clone();
        updated.title = "Updated Title".to_string();
        let result = db.update_block(&created.id, updated).await.unwrap();
        assert_eq!(result.title, "Updated Title");

        // Delete
        db.delete_block(&created.id).await.unwrap();
        let deleted = db.get_block(&created.id).await.unwrap();
        assert!(deleted.is_none());
    }
}
