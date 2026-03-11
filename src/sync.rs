use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::db::{Block, Database};
use crate::embeddings::{create_embedding_service, prepare_block_text, EmbeddingService};
use crate::indexer::extract_blocks_from_file;
use crate::watcher::{scan_vault, FileEvent, VaultWatcher};

/// Synchronizer handles indexing and keeping the database in sync with the vault
pub struct Synchronizer {
    db: Arc<RwLock<Database>>,
    config: Arc<Config>,
    watcher: Option<VaultWatcher>,
    embedding_service: Option<Arc<dyn EmbeddingService>>,
}

impl Synchronizer {
    /// Create a new synchronizer
    pub fn new(db: Arc<RwLock<Database>>, config: Arc<Config>) -> Result<Self> {
        let watcher = if config.sync.watch_for_changes {
            Some(VaultWatcher::new(&config.vault.path)?)
        } else {
            None
        };

        // Initialize embedding service if configured
        let embedding_service = create_embedding_service(&config.embedding)
            .ok()
            .map(Arc::from);

        if embedding_service.is_some() {
            info!("✅ Embedding service initialized");
        } else {
            info!("⚠️  Embedding service disabled");
        }

        Ok(Self {
            db,
            config,
            watcher,
            embedding_service,
        })
    }

    /// Generate embeddings for a batch of blocks
    async fn generate_embeddings(&self, blocks: &mut [Block]) -> Result<()> {
        if let Some(service) = &self.embedding_service {
            if blocks.is_empty() {
                return Ok(());
            }

            debug!("Generating embeddings for {} blocks", blocks.len());

            // Prepare texts for embedding
            let texts: Vec<String> = blocks
                .iter()
                .map(|b| prepare_block_text(&b.title, &b.content, 8000))
                .collect();

            // Generate embeddings
            match service.embed_batch(texts).await {
                Ok(embeddings) => {
                    // Assign embeddings and compute hashes for blocks
                    for (block, embedding) in blocks.iter_mut().zip(embeddings) {
                        block.embedding = Some(embedding);
                        block.content_hash = Some(block.compute_content_hash());
                    }
                    debug!("✅ Generated {} embeddings", blocks.len());
                }
                Err(e) => {
                    warn!("Failed to generate embeddings: {}. Continuing without embeddings.", e);
                }
            }
        }

        Ok(())
    }

    /// Perform initial indexing of the entire vault
    pub async fn initial_index(&self) -> Result<()> {
        if !self.config.sync.initial_indexing {
            info!("Initial indexing disabled, skipping");
            return Ok(());
        }

        info!("Starting initial vault indexing...");

        // Scan for all markdown files
        let files = scan_vault(&self.config.vault.path)?;
        info!("Found {} files to index", files.len());

        // Process files in batches
        let batch_size = self.config.sync.batch_size;
        let mut indexed_count = 0;
        let mut error_count = 0;

        for batch in files.chunks(batch_size) {
            let mut batch_blocks = Vec::new();

            // Extract blocks from each file in the batch
            for file_path in batch {
                match extract_blocks_from_file(file_path, &self.config.vault.path) {
                    Ok(blocks) => {
                        debug!(
                            "Extracted {} blocks from {}",
                            blocks.len(),
                            file_path.display()
                        );
                        batch_blocks.extend(blocks);
                    }
                    Err(e) => {
                        error!("Failed to extract blocks from {}: {}", file_path.display(), e);
                        error_count += 1;
                    }
                }
            }

            // Generate embeddings for the batch
            if !batch_blocks.is_empty() {
                if let Err(e) = self.generate_embeddings(&mut batch_blocks).await {
                    warn!("Failed to generate embeddings for batch: {}", e);
                }

                // Insert batch into database
                let db = self.db.write().await;
                for block in batch_blocks {
                    match db.create_block(block).await {
                        Ok(_) => indexed_count += 1,
                        Err(e) => {
                            error!("Failed to create block: {}", e);
                            error_count += 1;
                        }
                    }
                }
            }

            info!(
                "Indexed {}/{} files ({} blocks, {} errors)",
                indexed_count.min(files.len()),
                files.len(),
                indexed_count,
                error_count
            );
        }

        // Build backlink relationships
        self.update_backlinks().await?;

        info!(
            "Initial indexing complete: {} blocks indexed, {} errors",
            indexed_count, error_count
        );

        Ok(())
    }

    /// Update backlink relationships for all blocks
    async fn update_backlinks(&self) -> Result<()> {
        info!("Building backlink relationships...");

        // TODO: Temporarily disabled due to SurrealDB deserialization bug
        info!("⚠️  Skipping backlink building due to SurrealDB deserialization issue");
        return Ok(());

        #[allow(unreachable_code)]
        {
        let db = self.db.read().await;

        // Get all blocks (using a large limit instead of usize::MAX to avoid overflow)
        let all_blocks: Vec<Block> = db
            .search_blocks("", 100000)
            .await
            .context("Failed to get all blocks")?;

        // Build a map of file_path -> block_id for link resolution
        let mut file_to_block: HashMap<String, String> = HashMap::new();
        for block in &all_blocks {
            if block.level == 0 {
                // File blocks (level 0)
                file_to_block.insert(block.file_path.clone(), block.id.clone());
            }
        }

        // Build backlink map: target_block_id -> list of source_block_ids
        let mut backlinks: HashMap<String, Vec<String>> = HashMap::new();

        for block in &all_blocks {
            for link_target in &block.outgoing_links {
                // Try to resolve the link target to a block ID
                // Links in Obsidian are typically file names without extension
                let target_file = if link_target.ends_with(".md") {
                    link_target.clone()
                } else {
                    format!("{}.md", link_target)
                };

                if let Some(target_block_id) = file_to_block.get(&target_file) {
                    backlinks
                        .entry(target_block_id.clone())
                        .or_insert_with(Vec::new)
                        .push(block.id.clone());
                }
            }
        }

        // Update incoming_links for each block
        drop(db); // Release read lock
        let db = self.db.write().await;

        for (block_id, incoming) in backlinks {
            if let Ok(Some(mut block)) = db.get_block(&block_id).await {
                block.incoming_links = incoming;
                if let Err(e) = db.update_block(&block_id, block).await {
                    error!("Failed to update backlinks for block {}: {}", block_id, e);
                }
            }
        }

        info!("Backlink relationships updated");
        Ok(())
        }
    }

    /// Index a single file
    async fn index_file(&self, file_path: &Path) -> Result<()> {
        debug!("Indexing file: {}", file_path.display());

        // Extract blocks from the file
        let mut blocks = extract_blocks_from_file(file_path, &self.config.vault.path)
            .with_context(|| format!("Failed to extract blocks from {}", file_path.display()))?;

        // Generate embeddings for the blocks
        if let Err(e) = self.generate_embeddings(&mut blocks).await {
            warn!("Failed to generate embeddings for {}: {}", file_path.display(), e);
        }

        // Get relative path for database lookup
        let relative_path = file_path
            .strip_prefix(&self.config.vault.path)
            .context("File path is not within vault")?
            .to_string_lossy()
            .to_string();

        let db = self.db.write().await;

        // Delete existing blocks for this file
        let existing_blocks = db.get_blocks_by_file(&relative_path).await?;
        for block in existing_blocks {
            db.delete_block(&block.id).await?;
        }

        // Insert new blocks
        for block in blocks {
            db.create_block(block).await?;
        }

        info!("Indexed file: {}", file_path.display());

        Ok(())
    }

    /// Delete blocks for a file
    async fn delete_file_blocks(&self, file_path: &Path) -> Result<()> {
        debug!("Deleting blocks for file: {}", file_path.display());

        let relative_path = file_path
            .strip_prefix(&self.config.vault.path)
            .context("File path is not within vault")?
            .to_string_lossy()
            .to_string();

        let db = self.db.write().await;
        let blocks = db.get_blocks_by_file(&relative_path).await?;

        for block in blocks {
            db.delete_block(&block.id).await?;
        }

        info!("Deleted blocks for file: {}", file_path.display());

        Ok(())
    }

    /// Run the synchronizer (process file events)
    pub async fn run(mut self) -> Result<()> {
        if let Some(mut watcher) = self.watcher.take() {
            info!("Starting file watcher event loop...");

            while let Some(event) = watcher.next_event().await {
                match event {
                    FileEvent::Changed(path) => {
                        if let Err(e) = self.index_file(&path).await {
                            error!("Failed to index file {}: {}", path.display(), e);
                        }
                        // Update backlinks after each change
                        if let Err(e) = self.update_backlinks().await {
                            error!("Failed to update backlinks: {}", e);
                        }
                    }
                    FileEvent::Deleted(path) => {
                        if let Err(e) = self.delete_file_blocks(&path).await {
                            error!("Failed to delete blocks for {}: {}", path.display(), e);
                        }
                        // Update backlinks after deletion
                        if let Err(e) = self.update_backlinks().await {
                            error!("Failed to update backlinks: {}", e);
                        }
                    }
                }
            }
        } else {
            warn!("File watching disabled, synchronizer is idle");
        }

        Ok(())
    }

    /// Run initial indexing in the background
    pub async fn run_with_initial_index(self) -> Result<()> {
        // Perform initial indexing
        self.initial_index().await?;

        // Start watching for changes
        self.run().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn create_test_config(vault_path: PathBuf, db_path: PathBuf) -> Config {
        Config {
            vault: VaultConfig { path: vault_path },
            database: DatabaseConfig { path: db_path },
            embedding: EmbeddingConfig {
                provider: EmbeddingProvider::OpenAi,
                model: "test".to_string(),
                dimensions: 1536,
                api_key: Some("test".to_string()),
                api_base: Some("https://api.openai.com/v1".to_string()),
                model_cache: None,
            },
            reranking: RerankingConfig {
                enabled: false,
                provider: None,
                model: None,
                api_key: None,
                api_base: None,
                model_cache: None,
                top_n: 10,
            },
            sync: SyncConfig {
                watch_for_changes: false,
                initial_indexing: true,
                batch_size: 10,
            },
            graph: GraphConfig {
                extract_links: true,
                extract_backlinks: true,
                extract_tags: true,
                extract_mentions: true,
            },
        }
    }

    #[tokio::test]
    async fn test_initial_indexing() {
        let temp_dir = tempdir().unwrap();
        let vault_path = temp_dir.path().join("vault");
        let db_path = temp_dir.path().join("test.db");

        fs::create_dir(&vault_path).unwrap();

        // Create test markdown files
        fs::write(
            vault_path.join("note1.md"),
            "# Note 1\n\nSome content with [[note2]].",
        )
        .unwrap();
        fs::write(vault_path.join("note2.md"), "# Note 2\n\nMore content.").unwrap();

        let config = Arc::new(create_test_config(vault_path, db_path.clone()));
        let db = Arc::new(RwLock::new(Database::new(&db_path).await.unwrap()));

        let sync = Synchronizer::new(db.clone(), config).unwrap();
        sync.initial_index().await.unwrap();

        // Check that blocks were created
        let db = db.read().await;
        let blocks = db.search_blocks("", 100000).await.unwrap();

        // Should have blocks from both files
        assert!(blocks.len() >= 2);

        // Check that backlinks were created
        let note2_blocks: Vec<&Block> = blocks
            .iter()
            .filter(|b| b.file_path == "note2.md" && b.level == 0)
            .collect();

        assert!(!note2_blocks.is_empty());
        // note2 should have an incoming link from note1
        assert!(!note2_blocks[0].incoming_links.is_empty());
    }
}
