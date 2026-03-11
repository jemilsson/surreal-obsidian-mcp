use anyhow::Result;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::transport::stdio;
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::config::Config;
use crate::db::{Block, Database};
use crate::embeddings::{create_embedding_service, prepare_block_text, EmbeddingService};
use crate::indexer;
use crate::reranking::{create_reranking_service, RerankingService};
use crate::writer;

/// MCP Server for Obsidian vault indexing
#[derive(Clone)]
pub struct McpServer {
    db: Arc<RwLock<Database>>,
    #[allow(dead_code)]
    config: Arc<Config>,
    embedding_service: Option<Arc<dyn EmbeddingService>>,
    reranking_service: Option<Arc<dyn RerankingService>>,
    tool_router: ToolRouter<Self>,
}

// Tool input schemas
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct SearchBlocksInput {
    /// Search query to match against block titles and content
    pub query: String,
    /// Maximum number of results to return
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetBlockInput {
    /// Block ID to retrieve
    pub id: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetBlocksByFileInput {
    /// File path relative to vault root
    pub file_path: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetChildrenInput {
    /// Parent block ID
    pub parent_id: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct SearchSimilarInput {
    /// Query text to find semantically similar blocks
    pub query: String,
    /// Maximum number of results to return
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Depth of graph expansion (0=none, 1=direct links, 2=links of links, etc.)
    #[serde(default)]
    pub expand: u8,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct CreateBlockInput {
    /// File path relative to vault root (e.g., "folder/note.md")
    pub file_path: String,
    /// Block title (filename for level 0, heading text for level 1-6)
    pub title: String,
    /// Block content
    pub content: String,
    /// Block level (0 for file, 1-6 for headings)
    #[serde(default)]
    pub level: u8,
    /// Parent block ID (optional, for creating headings within existing files)
    pub parent_id: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct UpdateBlockInput {
    /// Block ID to update
    pub id: String,
    /// New title (optional)
    pub title: Option<String>,
    /// New content (optional)
    pub content: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteBlockInput {
    /// Block ID to delete
    pub id: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct AppendToBlockInput {
    /// Block ID to append to
    pub id: String,
    /// Content to append
    pub content: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetLinkedBlocksInput {
    /// Block ID to get outgoing links from
    pub id: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetBacklinksInput {
    /// Block ID to get incoming links to
    pub id: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct FindByTagInput {
    /// Tag to search for (without the # prefix)
    pub tag: String,
    /// Maximum number of results to return
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct FindConnectionPathInput {
    /// Starting block ID
    pub from_id: String,
    /// Target block ID
    pub to_id: String,
    /// Maximum path depth to search (default 5)
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
}

fn default_limit() -> usize {
    10
}

fn default_max_depth() -> usize {
    5
}

/// Expand the graph from a starting block to the specified depth
/// Returns all blocks discovered during expansion (excluding the starting block)
async fn expand_block_graph(
    db: &Database,
    start_id: &str,
    depth: u8,
) -> Result<Vec<Block>, anyhow::Error> {
    use std::collections::{HashSet, VecDeque};

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut expanded_blocks = Vec::new();

    // Start BFS
    queue.push_back((start_id.to_string(), 0u8));
    visited.insert(start_id.to_string());

    while let Some((block_id, current_depth)) = queue.pop_front() {
        // Stop if we've reached the max depth
        if current_depth >= depth {
            continue;
        }

        // Get the block to find its neighbors
        let block = match db.get_block(&block_id).await? {
            Some(b) => b,
            None => continue,
        };

        // Collect all neighbor IDs
        let mut neighbor_ids = HashSet::new();

        // 1. Outgoing links (wiki-links)
        for link_id in &block.outgoing_links {
            neighbor_ids.insert(link_id.clone());
        }

        // 2. Incoming links (backlinks)
        for link_id in &block.incoming_links {
            neighbor_ids.insert(link_id.clone());
        }

        // 3. Parent block
        if let Some(parent_id) = &block.parent_id {
            neighbor_ids.insert(parent_id.clone());
        }

        // 4. Child blocks
        for child_id in &block.children_ids {
            neighbor_ids.insert(child_id.clone());
        }

        // Process neighbors
        for neighbor_id in neighbor_ids {
            if !visited.contains(&neighbor_id) {
                visited.insert(neighbor_id.clone());
                queue.push_back((neighbor_id.clone(), current_depth + 1));

                // Fetch and add the neighbor block
                if let Some(neighbor_block) = db.get_block(&neighbor_id).await? {
                    expanded_blocks.push(neighbor_block);
                }
            }
        }
    }

    Ok(expanded_blocks)
}

impl McpServer {
    /// List all available tools
    pub fn list_tools(&self) -> Vec<Tool> {
        self.tool_router.list_tools()
    }

    /// Call a tool by name with arguments
    pub async fn call_tool(&self, request: CallToolRequest) -> Result<CallToolResult, McpError> {
        self.tool_router.call_tool(self, request).await
    }

    /// Automatically re-index a file when database inconsistencies are detected
    async fn auto_reindex_file(&self, file_path: &str) -> Result<(), McpError> {
        info!("⚠️  Auto-reindexing file due to database inconsistency: {}", file_path);

        // Build absolute path
        let absolute_path = std::path::Path::new(&self.config.vault.path).join(file_path);

        // Re-extract blocks from the file
        let blocks = indexer::extract_blocks_from_file(&absolute_path, &self.config.vault.path)
            .map_err(|e| McpError::internal_error(format!("Failed to extract blocks: {}", e), None))?;

        let db = self.db.write().await;

        // Delete all existing blocks for this file
        db.delete_blocks_by_file(file_path)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to delete old blocks: {}", e), None))?;

        // Insert all blocks with embeddings
        for mut block in blocks {
            if let Some(embedding_service) = &self.embedding_service {
                let hash = block.compute_content_hash();
                let text = prepare_block_text(&block.title, &block.content, 8000);
                if let Ok(embedding) = embedding_service.embed(text).await {
                    block.embedding = Some(embedding);
                    block.content_hash = Some(hash);
                }
            }

            db.create_block(block)
                .await
                .map_err(|e| McpError::internal_error(format!("Failed to create block: {}", e), None))?;
        }

        drop(db);

        info!("✅ Successfully re-indexed file: {}", file_path);
        Ok(())
    }

    /// Create a new MCP server
    pub fn new(db: Arc<RwLock<Database>>, config: Arc<Config>) -> Self {
        // Initialize embedding service if configured
        let embedding_service = create_embedding_service(&config.embedding)
            .ok()
            .map(Arc::from);

        // Initialize reranking service if configured
        let reranking_service = create_reranking_service(&config.reranking)
            .ok()
            .map(Arc::from);

        if reranking_service.is_some() {
            info!("✅ Reranking service initialized");
        }

        Self {
            db,
            config,
            embedding_service,
            reranking_service,
            tool_router: Self::tool_router(),
        }
    }

    /// Run the MCP server with stdio transport
    pub async fn run(self) -> Result<()> {
        info!("Starting MCP server on stdio");

        let service = self
            .serve(stdio())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start MCP server: {}", e))?;

        service
            .waiting()
            .await
            .map_err(|e| anyhow::anyhow!("MCP server error: {}", e))?;

        Ok(())
    }
}

#[tool_router]
impl McpServer {
    /// Search blocks by content (title or body text)
    #[tool(description = "Search blocks by content (title or body text)")]
    async fn search_blocks(
        &self,
        params: Parameters<SearchBlocksInput>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.read().await;
        let blocks = db
            .search_blocks(&params.0.query, params.0.limit)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let text = format!(
            "Found {} blocks matching '{}':\n\n{}",
            blocks.len(),
            params.0.query,
            blocks
                .iter()
                .map(|b| format!(
                    "- [{}] {}\n  File: {}\n  Content: {}\n",
                    b.id,
                    b.title,
                    b.file_path,
                    b.content
                        .lines()
                        .next()
                        .unwrap_or("")
                        .chars()
                        .take(100)
                        .collect::<String>()
                ))
                .collect::<String>()
        );

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Get a specific block by ID
    #[tool(description = "Get a specific block by ID")]
    async fn get_block(
        &self,
        params: Parameters<GetBlockInput>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.read().await;
        let block = db
            .get_block(&params.0.id)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        match block {
            Some(b) => {
                let text = serde_json::to_string_pretty(&b)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            None => Err(McpError::invalid_request(
                format!("Block not found: {}", params.0.id),
                None,
            )),
        }
    }

    /// Get all blocks for a specific file path (relative to vault root)
    #[tool(description = "Get all blocks for a specific file path (relative to vault root)")]
    async fn get_blocks_by_file(
        &self,
        params: Parameters<GetBlocksByFileInput>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.read().await;
        let blocks = db
            .get_blocks_by_file(&params.0.file_path)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let text = format!(
            "Found {} blocks in file '{}':\n\n{}",
            blocks.len(),
            params.0.file_path,
            serde_json::to_string_pretty(&blocks)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
        );

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Get all file blocks (level 0) in the vault
    #[tool(description = "Get all file blocks (level 0) in the vault")]
    async fn get_all_files(&self) -> Result<CallToolResult, McpError> {
        let db = self.db.read().await;
        let files = db
            .get_all_files()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let text = format!(
            "Found {} files:\n\n{}",
            files.len(),
            files
                .iter()
                .map(|f| format!("- {} ({})\n", f.title, f.file_path))
                .collect::<String>()
        );

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Get child blocks of a specific block
    #[tool(description = "Get child blocks of a specific block")]
    async fn get_children(
        &self,
        params: Parameters<GetChildrenInput>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.read().await;
        let children = db
            .get_children(&params.0.parent_id)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let text = format!(
            "Found {} children for block '{}':\n\n{}",
            children.len(),
            params.0.parent_id,
            serde_json::to_string_pretty(&children)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
        );

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Search for semantically similar blocks using vector embeddings
    #[tool(description = "Search for semantically similar blocks using vector embeddings")]
    async fn search_similar(
        &self,
        params: Parameters<SearchSimilarInput>,
    ) -> Result<CallToolResult, McpError> {
        // Check if embedding service is available
        let embedding_service = self.embedding_service.as_ref().ok_or_else(|| {
            McpError::invalid_request(
                "Embedding service not configured. Please configure an embedding provider in config.json.".to_string(),
                None,
            )
        })?;

        // Generate embedding for the query
        let query_text = prepare_block_text(&params.0.query, "", 8000);
        let query_embedding = embedding_service
            .embed(query_text)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to generate query embedding: {}", e), None))?;

        // Search for similar blocks (get more results if reranking is enabled)
        let initial_limit = if self.reranking_service.is_some() {
            params.0.limit * 3 // Get 3x results for reranking
        } else {
            params.0.limit
        };

        let db = self.db.read().await;
        let mut blocks = db
            .search_similar(query_embedding, initial_limit)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        drop(db);

        // Apply reranking if available
        let core_results: Vec<Block> = if let Some(reranker) = &self.reranking_service {
            let ranked_results = reranker
                .rerank(&params.0.query, blocks)
                .await
                .map_err(|e| McpError::internal_error(format!("Reranking failed: {}", e), None))?;
            ranked_results.into_iter().map(|r| r.block).collect()
        } else {
            // No reranking - truncate to requested limit
            blocks.truncate(params.0.limit);
            blocks
        };

        // Perform graph expansion if requested
        if params.0.expand > 0 {
            let db = self.db.read().await;
            let mut expanded_blocks = std::collections::HashSet::new();
            let mut core_ids = std::collections::HashSet::new();

            // Mark core results
            for block in &core_results {
                core_ids.insert(block.id.clone());
            }

            // Expand from each core result
            for block in &core_results {
                let expanded = expand_block_graph(&db, &block.id, params.0.expand).await
                    .map_err(|e| McpError::internal_error(format!("Graph expansion failed: {}", e), None))?;

                for exp_block in expanded {
                    // Only add if not already in core results
                    if !core_ids.contains(&exp_block.id) {
                        expanded_blocks.insert(exp_block.id.clone());
                    }
                }
            }

            // Fetch expanded blocks
            let mut expanded_block_list = Vec::new();
            for block_id in expanded_blocks {
                if let Some(block) = db.get_block(&block_id).await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))? {
                    expanded_block_list.push(block);
                }
            }
            drop(db);

            // Format results with core and expanded sections
            let mut text = format!(
                "Found {} semantically similar blocks for '{}' (expanded to depth {}):\n\n## Core Results ({})\n\n",
                core_results.len(),
                params.0.query,
                params.0.expand,
                core_results.len()
            );

            for b in &core_results {
                text.push_str(&format!(
                    "- [{}] {}\n  File: {}\n  Content: {}\n\n",
                    b.id,
                    b.title,
                    b.file_path,
                    b.content
                        .lines()
                        .next()
                        .unwrap_or("")
                        .chars()
                        .take(100)
                        .collect::<String>()
                ));
            }

            if !expanded_block_list.is_empty() {
                text.push_str(&format!("\n## Related via Graph Expansion ({})\n\n", expanded_block_list.len()));
                for b in &expanded_block_list {
                    text.push_str(&format!(
                        "- [{}] {}\n  File: {}\n  Content: {}\n\n",
                        b.id,
                        b.title,
                        b.file_path,
                        b.content
                            .lines()
                            .next()
                            .unwrap_or("")
                            .chars()
                            .take(100)
                            .collect::<String>()
                    ));
                }
            }

            return Ok(CallToolResult::success(vec![Content::text(text)]));
        }

        // No expansion - just return core results
        let text = format!(
            "Found {} semantically similar blocks for '{}':\n\n{}",
            core_results.len(),
            params.0.query,
            core_results
                .iter()
                .map(|b| format!(
                    "- [{}] {}\n  File: {}\n  Content: {}\n",
                    b.id,
                    b.title,
                    b.file_path,
                    b.content
                        .lines()
                        .next()
                        .unwrap_or("")
                        .chars()
                        .take(100)
                        .collect::<String>()
                ))
                .collect::<String>()
        );

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Create a new block (file or heading)
    #[tool(description = "Create a new block (file or heading) in the vault")]
    async fn create_block(
        &self,
        params: Parameters<CreateBlockInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;

        // Validate level
        if input.level > 6 {
            return Err(McpError::invalid_request(
                "Block level must be between 0 (file) and 6 (H6 heading)".to_string(),
                None,
            ));
        }

        // Create the block
        let mut block = Block::new(
            input.level,
            input.title,
            input.content,
            input.file_path.clone(),
        );

        block.parent_id = input.parent_id;

        // Compute content hash and generate embedding if service is available
        if let Some(embedding_service) = &self.embedding_service {
            let hash = block.compute_content_hash();
            let text = prepare_block_text(&block.title, &block.content, 8000);
            match embedding_service.embed(text).await {
                Ok(embedding) => {
                    block.embedding = Some(embedding);
                    block.content_hash = Some(hash);
                }
                Err(e) => {
                    info!("Failed to generate embedding for new block: {}", e);
                }
            }
        }

        // Save to database
        let db = self.db.write().await;
        let created_block = db
            .create_block(block)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to create block: {}", e), None))?;

        // Get all blocks for this file to reconstruct it
        let file_blocks = db
            .get_blocks_by_file(&input.file_path)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to get file blocks: {}", e), None))?;

        drop(db);

        // Write the file
        writer::write_file_from_blocks(&self.config.vault.path, &input.file_path, &file_blocks)
            .map_err(|e| McpError::internal_error(format!("Failed to write file: {}", e), None))?;

        let text = format!(
            "✅ Created block [{}] in file '{}'\n\n{}",
            created_block.id,
            input.file_path,
            serde_json::to_string_pretty(&created_block)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
        );

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Update an existing block's content or title
    #[tool(description = "Update an existing block's content or title")]
    async fn update_block(
        &self,
        params: Parameters<UpdateBlockInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;

        let db = self.db.write().await;

        // Get the existing block
        let block_result = db.get_block(&input.id).await;

        let (mut block, file_path) = match block_result {
            Ok(Some(b)) => {
                let fp = b.file_path.clone();
                (b, fp)
            }
            Ok(None) | Err(_) => {
                // Block not found - find the file and re-index it
                drop(db);

                let db_read = self.db.read().await;
                let files = db_read.get_all_files().await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                let mut target_file = None;
                for file in files {
                    let blocks = db_read.get_blocks_by_file(&file.file_path).await
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                    if blocks.iter().any(|b| b.id == input.id) {
                        target_file = Some(file.file_path.clone());
                        break;
                    }
                }
                drop(db_read);

                if let Some(fp) = target_file {
                    // Re-index the file
                    self.auto_reindex_file(&fp).await?;

                    // Retry getting the block
                    let db = self.db.write().await;
                    let b = db.get_block(&input.id).await
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?
                        .ok_or_else(|| McpError::invalid_request(
                            format!("Block not found even after re-indexing: {}", input.id),
                            None
                        ))?;
                    let fp2 = b.file_path.clone();
                    drop(db);
                    (b, fp2)
                } else {
                    return Err(McpError::invalid_request(format!("Block not found: {}", input.id), None));
                }
            }
        };

        let db = self.db.write().await;

        // Apply updates
        if let Some(title) = input.title {
            block.title = title;
        }

        if let Some(content) = input.content {
            block.content = content;
        }

        // Regenerate embedding only if content changed (based on hash)
        if let Some(embedding_service) = &self.embedding_service {
            let old_hash = block.content_hash.as_deref();
            if block.content_changed(old_hash) {
                let hash = block.compute_content_hash();
                let text = prepare_block_text(&block.title, &block.content, 8000);
                match embedding_service.embed(text).await {
                    Ok(embedding) => {
                        block.embedding = Some(embedding);
                        block.content_hash = Some(hash);
                        info!("Regenerated embedding for updated block (content changed)");
                    }
                    Err(e) => {
                        info!("Failed to regenerate embedding for updated block: {}", e);
                    }
                }
            } else {
                info!("Content hash unchanged, skipping embedding regeneration");
            }
        }

        // Update in database
        let updated_block = db
            .update_block(&input.id, block)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to update block: {}", e), None))?;

        // Get all blocks for this file to reconstruct it
        let file_blocks = db
            .get_blocks_by_file(&file_path)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to get file blocks: {}", e), None))?;

        drop(db);

        // Write the file
        writer::write_file_from_blocks(&self.config.vault.path, &file_path, &file_blocks)
            .map_err(|e| McpError::internal_error(format!("Failed to write file: {}", e), None))?;

        let text = format!(
            "✅ Updated block [{}]\n\n{}",
            input.id,
            serde_json::to_string_pretty(&updated_block)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
        );

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Delete a block
    #[tool(description = "Delete a block from the vault")]
    async fn delete_block(
        &self,
        params: Parameters<DeleteBlockInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;

        let db = self.db.write().await;

        // Get the block to find its file path
        let block_result = db.get_block(&input.id).await;

        let block = match block_result {
            Ok(Some(b)) => b,
            Ok(None) | Err(_) => {
                // Block not found - find the file and re-index it
                drop(db);

                let db_read = self.db.read().await;
                let files = db_read.get_all_files().await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                let mut target_file = None;
                for file in files {
                    let blocks = db_read.get_blocks_by_file(&file.file_path).await
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                    if blocks.iter().any(|b2| b2.id == input.id) {
                        target_file = Some(file.file_path.clone());
                        break;
                    }
                }
                drop(db_read);

                if let Some(fp) = target_file {
                    // Re-index the file
                    self.auto_reindex_file(&fp).await?;

                    // Retry getting the block
                    let db = self.db.write().await;
                    let b = db.get_block(&input.id).await
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?
                        .ok_or_else(|| McpError::invalid_request(
                            format!("Block not found even after re-indexing: {}", input.id),
                            None
                        ))?;
                    drop(db);
                    b
                } else {
                    return Err(McpError::invalid_request(format!("Block not found: {}", input.id), None));
                }
            }
        };

        let db = self.db.write().await;

        let file_path = block.file_path.clone();
        let is_file = block.level == 0;

        // Delete the block
        db.delete_block(&input.id)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to delete block: {}", e), None))?;

        if is_file {
            // If it's a file block, delete the entire file and all its blocks
            let file_blocks = db
                .get_blocks_by_file(&file_path)
                .await
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;

            for b in file_blocks {
                db.delete_block(&b.id).await.map_err(|e| {
                    McpError::internal_error(format!("Failed to delete block: {}", e), None)
                })?;
            }

            drop(db);

            // Delete the file
            writer::delete_file(&self.config.vault.path, &file_path)
                .map_err(|e| McpError::internal_error(format!("Failed to delete file: {}", e), None))?;

            Ok(CallToolResult::success(vec![Content::text(format!(
                "✅ Deleted file: {}",
                file_path
            ))]))
        } else {
            // Get remaining blocks for this file to reconstruct it
            let file_blocks = db
                .get_blocks_by_file(&file_path)
                .await
                .map_err(|e| McpError::internal_error(format!("Failed to get file blocks: {}", e), None))?;

            drop(db);

            // Reconstruct and write the file without the deleted block
            if !file_blocks.is_empty() {
                writer::write_file_from_blocks(&self.config.vault.path, &file_path, &file_blocks)
                    .map_err(|e| McpError::internal_error(format!("Failed to write file: {}", e), None))?;
            }

            Ok(CallToolResult::success(vec![Content::text(format!(
                "✅ Deleted block [{}] from file '{}'",
                input.id, file_path
            ))]))
        }
    }

    /// Append content to an existing block
    #[tool(description = "Append content to an existing block")]
    async fn append_to_block(
        &self,
        params: Parameters<AppendToBlockInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;

        let db = self.db.write().await;

        // Get the existing block
        let mut block = db
            .get_block(&input.id)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .ok_or_else(|| McpError::invalid_request(format!("Block not found: {}", input.id), None))?;

        let file_path = block.file_path.clone();

        // Append content
        if !block.content.is_empty() && !block.content.ends_with('\n') {
            block.content.push('\n');
        }
        block.content.push_str(&input.content);

        // Regenerate embedding with new hash
        if let Some(embedding_service) = &self.embedding_service {
            let hash = block.compute_content_hash();
            let text = prepare_block_text(&block.title, &block.content, 8000);
            match embedding_service.embed(text).await {
                Ok(embedding) => {
                    block.embedding = Some(embedding);
                    block.content_hash = Some(hash);
                }
                Err(e) => {
                    info!("Failed to regenerate embedding for appended block: {}", e);
                }
            }
        }

        // Update in database
        let updated_block = db
            .update_block(&input.id, block)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to update block: {}", e), None))?;

        // Get all blocks for this file to reconstruct it
        let file_blocks = db
            .get_blocks_by_file(&file_path)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to get file blocks: {}", e), None))?;

        drop(db);

        // Write the file
        writer::write_file_from_blocks(&self.config.vault.path, &file_path, &file_blocks)
            .map_err(|e| McpError::internal_error(format!("Failed to write file: {}", e), None))?;

        let text = format!(
            "✅ Appended to block [{}]\n\nUpdated content preview:\n{}",
            input.id,
            updated_block
                .content
                .lines()
                .take(5)
                .collect::<Vec<_>>()
                .join("\n")
        );

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Get blocks that this block links to (outgoing links)
    #[tool(description = "Get blocks that this block links to (outgoing wiki-links)")]
    async fn get_linked_blocks(
        &self,
        params: Parameters<GetLinkedBlocksInput>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.read().await;
        let linked_blocks = db
            .get_linked_blocks(&params.0.id)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let text = format!(
            "Found {} blocks linked from [{}]:\n\n{}",
            linked_blocks.len(),
            params.0.id,
            if linked_blocks.is_empty() {
                "No outgoing links.".to_string()
            } else {
                linked_blocks
                    .iter()
                    .map(|b| format!(
                        "- [{}] {}\n  File: {}\n",
                        b.id,
                        b.title,
                        b.file_path
                    ))
                    .collect::<String>()
            }
        );

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Get blocks that link to this block (backlinks)
    #[tool(description = "Get blocks that link to this block (incoming wiki-links/backlinks)")]
    async fn get_backlinks(
        &self,
        params: Parameters<GetBacklinksInput>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.read().await;
        let backlinks = db
            .get_backlinks(&params.0.id)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let text = format!(
            "Found {} blocks linking to [{}]:\n\n{}",
            backlinks.len(),
            params.0.id,
            if backlinks.is_empty() {
                "No backlinks.".to_string()
            } else {
                backlinks
                    .iter()
                    .map(|b| format!(
                        "- [{}] {}\n  File: {}\n",
                        b.id,
                        b.title,
                        b.file_path
                    ))
                    .collect::<String>()
            }
        );

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Find blocks by tag
    #[tool(description = "Find all blocks with a specific tag")]
    async fn find_by_tag(
        &self,
        params: Parameters<FindByTagInput>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.read().await;
        let blocks = db
            .find_by_tag(&params.0.tag, params.0.limit)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let text = format!(
            "Found {} blocks with tag #{}:\n\n{}",
            blocks.len(),
            params.0.tag,
            if blocks.is_empty() {
                "No blocks with this tag.".to_string()
            } else {
                blocks
                    .iter()
                    .map(|b| format!(
                        "- [{}] {}\n  File: {}\n  Tags: {}\n",
                        b.id,
                        b.title,
                        b.file_path,
                        b.tags.join(", ")
                    ))
                    .collect::<String>()
            }
        );

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Find connection path between two blocks via wiki-links
    #[tool(description = "Find a connection path between two blocks via wiki-links (BFS search)")]
    async fn find_connection_path(
        &self,
        params: Parameters<FindConnectionPathInput>,
    ) -> Result<CallToolResult, McpError> {
        let input = params.0;

        // Validate blocks exist
        let db = self.db.read().await;
        let from_block = db
            .get_block(&input.from_id)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .ok_or_else(|| McpError::invalid_request(format!("Source block not found: {}", input.from_id), None))?;

        let to_block = db
            .get_block(&input.to_id)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .ok_or_else(|| McpError::invalid_request(format!("Target block not found: {}", input.to_id), None))?;

        // BFS to find shortest path
        use std::collections::{HashMap, VecDeque};

        let mut queue = VecDeque::new();
        let mut visited = std::collections::HashSet::new();
        let mut parent: HashMap<String, String> = HashMap::new();

        queue.push_back(input.from_id.clone());
        visited.insert(input.from_id.clone());

        let mut found = false;
        let mut depth = 0;

        while !queue.is_empty() && depth < input.max_depth {
            let level_size = queue.len();

            for _ in 0..level_size {
                let current_id = queue.pop_front().unwrap();

                if current_id == input.to_id {
                    found = true;
                    break;
                }

                // Get linked blocks
                let linked = db
                    .get_linked_blocks(&current_id)
                    .await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                for link in linked {
                    if !visited.contains(&link.id) {
                        visited.insert(link.id.clone());
                        parent.insert(link.id.clone(), current_id.clone());
                        queue.push_back(link.id);
                    }
                }
            }

            if found {
                break;
            }

            depth += 1;
        }

        if !found {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No connection path found between [{}] and [{}] within {} hops.",
                from_block.title, to_block.title, input.max_depth
            ))]));
        }

        // Reconstruct path
        let mut path = Vec::new();
        let mut current = input.to_id.clone();

        while current != input.from_id {
            path.push(current.clone());
            current = parent.get(&current).unwrap().clone();
        }
        path.push(input.from_id.clone());
        path.reverse();

        // Get block details for path
        let mut path_blocks = Vec::new();
        for id in &path {
            if let Some(block) = db.get_block(id).await.map_err(|e| McpError::internal_error(e.to_string(), None))? {
                path_blocks.push(block);
            }
        }

        let text = format!(
            "Found connection path ({} hops):\n\n{}",
            path.len() - 1,
            path_blocks
                .iter()
                .enumerate()
                .map(|(i, b)| {
                    let arrow = if i < path_blocks.len() - 1 { " →" } else { "" };
                    format!("{}. [{}] {} ({}){}\n", i + 1, b.id, b.title, b.file_path, arrow)
                })
                .collect::<String>()
        );

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

}

// Implement the server handler
#[tool_handler]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "MCP server for indexing and searching Obsidian vaults using SurrealDB.\n\
                 Supports searching blocks, retrieving specific blocks, exploring file hierarchies,\n\
                 creating/updating/deleting blocks for LLM communication through notes,\n\
                 and graph traversal through wiki-links, backlinks, and tags."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
