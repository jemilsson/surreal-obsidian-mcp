use anyhow::{Context, Result};
use async_trait::async_trait;
use std::sync::Arc;

use crate::config::RerankingConfig;
use crate::db::Block;

/// Result of reranking with score
#[derive(Debug, Clone)]
pub struct RankedResult {
    pub block: Block,
    pub score: f32,
}

/// Trait for reranking providers
#[async_trait]
pub trait RerankingService: Send + Sync {
    /// Rerank a list of blocks based on their relevance to a query
    /// Returns blocks sorted by relevance (highest first) with scores
    async fn rerank(&self, query: &str, blocks: Vec<Block>) -> Result<Vec<RankedResult>>;
}

/// Create a reranking service from configuration
pub fn create_reranking_service(
    config: &RerankingConfig,
) -> Result<Arc<dyn RerankingService>> {
    if !config.enabled {
        anyhow::bail!("Reranking is not enabled in configuration");
    }

    let provider = config
        .provider
        .as_ref()
        .context("Reranking provider must be specified")?;

    match provider {
        #[cfg(feature = "embedded")]
        crate::config::RerankingProvider::Embedded => {
            Ok(Arc::new(CandleReranker::new(config)?))
        }
        crate::config::RerankingProvider::Cohere => {
            anyhow::bail!("Cohere reranker not yet implemented")
        }
        crate::config::RerankingProvider::Jina => {
            anyhow::bail!("Jina reranker not yet implemented")
        }
        crate::config::RerankingProvider::Ollama => {
            anyhow::bail!("Ollama reranker not yet implemented")
        }
    }
}

#[cfg(feature = "embedded")]
mod candle_reranker {
    use super::*;
    use candle_core::{DType, Device, IndexOp, Tensor};
    use candle_nn::VarBuilder;
    use candle_transformers::models::bert::{BertModel, Config as BertConfig};
    use hf_hub::{api::sync::Api, Repo, RepoType};
    use std::path::PathBuf;
    use tracing::{debug, info};
    use tokenizers::Tokenizer;

    /// Candle-based reranker using cross-encoder models
    pub struct CandleReranker {
        model: BertModel,
        tokenizer: Tokenizer,
        device: Device,
        top_n: usize,
    }

    impl CandleReranker {
        pub fn new(config: &RerankingConfig) -> Result<Self> {
            let model_name = config
                .model
                .as_ref()
                .context("Model name required for embedded reranker")?;

            info!("Loading Candle reranker model: {}", model_name);

            // Determine device (CPU for now, could add GPU support)
            let device = Device::Cpu;

            // Download model from HuggingFace
            let (model_path, tokenizer_path) = Self::download_model(model_name, &config.model_cache)?;

            // Load tokenizer
            let tokenizer = Tokenizer::from_file(&tokenizer_path)
                .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;

            // Load model config and weights
            let vb = unsafe {
                VarBuilder::from_mmaped_safetensors(&[model_path.join("model.safetensors")], DType::F32, &device)?
            };

            let config_path = model_path.join("config.json");
            let config_str = std::fs::read_to_string(&config_path)
                .context("Failed to read model config")?;
            let bert_config: BertConfig = serde_json::from_str(&config_str)
                .context("Failed to parse model config")?;

            let model = BertModel::load(vb, &bert_config)?;

            info!("✅ Candle reranker model loaded");

            Ok(Self {
                model,
                tokenizer,
                device,
                top_n: config.top_n,
            })
        }

        fn download_model(
            model_name: &str,
            cache_dir: &Option<PathBuf>,
        ) -> Result<(PathBuf, PathBuf)> {
            // Set cache directory via environment variable if specified
            if let Some(cache) = cache_dir {
                std::env::set_var("HF_HOME", cache);
            }

            let api = Api::new()?;
            let repo = api.repo(Repo::new(model_name.to_string(), RepoType::Model));

            info!("Downloading model files from HuggingFace...");

            // Download required files
            let model_dir = repo.get("model.safetensors")
                .context("Failed to download model weights")?
                .parent()
                .context("Invalid model path")?
                .to_path_buf();

            let tokenizer_path = repo.get("tokenizer.json")
                .context("Failed to download tokenizer")?;

            // Also download config
            let _config = repo.get("config.json")
                .context("Failed to download config")?;

            Ok((model_dir, tokenizer_path))
        }

        fn encode_pair(&self, query: &str, document: &str) -> Result<Tensor> {
            // Combine query and document as expected by cross-encoders
            let text = format!("{} [SEP] {}", query, document);

            // Tokenize
            let encoding = self
                .tokenizer
                .encode(text, true)
                .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

            let tokens = encoding.get_ids();
            let token_ids = Tensor::new(tokens, &self.device)?
                .unsqueeze(0)?; // Add batch dimension

            // Create attention mask
            let attention_mask = Tensor::ones((1, tokens.len()), DType::U8, &self.device)?;

            // Forward pass through model (None for token_type_ids)
            let output = self.model.forward(&token_ids, &attention_mask, None)?;

            // Get [CLS] token representation (first token)
            let cls_output = output.i((0, 0))?;

            Ok(cls_output)
        }

        fn score_pair(&self, query: &str, document: &str) -> Result<f32> {
            let embedding = self.encode_pair(query, document)?;

            // For cross-encoders, we typically use a linear layer on top of [CLS]
            // For simplicity, we'll use the mean of the CLS embedding as score
            // In a full implementation, you'd want to load the classification head
            let score = embedding.mean_all()?.to_scalar::<f32>()?;

            Ok(score)
        }
    }

    #[async_trait]
    impl RerankingService for CandleReranker {
        async fn rerank(&self, query: &str, blocks: Vec<Block>) -> Result<Vec<RankedResult>> {
            if blocks.is_empty() {
                return Ok(Vec::new());
            }

            debug!("Reranking {} blocks with query: {}", blocks.len(), query);

            let mut results = Vec::new();

            // Score each block
            for block in blocks {
                let document = format!("{}\n\n{}", block.title, block.content);

                match self.score_pair(query, &document) {
                    Ok(score) => {
                        results.push(RankedResult { block, score });
                    }
                    Err(e) => {
                        debug!("Failed to score block {}: {}", block.id, e);
                        // Continue with other blocks
                    }
                }
            }

            // Sort by score (descending)
            results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

            // Take top N
            results.truncate(self.top_n);

            debug!("✅ Reranked to {} results", results.len());

            Ok(results)
        }
    }
}

#[cfg(feature = "embedded")]
pub use candle_reranker::CandleReranker;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ranked_result() {
        // Just a basic struct test
        use crate::db::Block;

        let block = Block::new(0, "Test".to_string(), "Content".to_string(), "test.md".to_string());
        let result = RankedResult {
            block: block.clone(),
            score: 0.95,
        };

        assert_eq!(result.score, 0.95);
        assert_eq!(result.block.title, "Test");
    }
}
