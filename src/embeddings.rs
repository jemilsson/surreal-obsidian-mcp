use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info};

use crate::config::{EmbeddingConfig, EmbeddingProvider};

/// Trait for embedding providers
#[async_trait]
pub trait EmbeddingService: Send + Sync {
    /// Generate embeddings for a batch of texts
    async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>>;

    /// Generate embedding for a single text
    async fn embed(&self, text: String) -> Result<Vec<f32>> {
        let results = self.embed_batch(vec![text]).await?;
        results.into_iter().next().context("No embedding returned")
    }

    /// Get the dimension of the embeddings
    fn dimensions(&self) -> usize;
}

/// Create an embedding service from configuration
pub fn create_embedding_service(config: &EmbeddingConfig) -> Result<Arc<dyn EmbeddingService>> {
    match config.provider {
        EmbeddingProvider::OpenAi | EmbeddingProvider::OpenAiCompatible => {
            Ok(Arc::new(OpenAiEmbeddings::new(config)?))
        }
        EmbeddingProvider::Ollama => Ok(Arc::new(OllamaEmbeddings::new(config)?)),
    }
}

/// OpenAI-compatible embedding provider
pub struct OpenAiEmbeddings {
    client: Client,
    api_base: String,
    api_key: String,
    model: String,
    dimensions: usize,
}

impl OpenAiEmbeddings {
    pub fn new(config: &EmbeddingConfig) -> Result<Self> {
        let api_key = config
            .api_key
            .as_ref()
            .context("API key required for OpenAI embeddings")?
            .clone();

        let api_base = config
            .api_base
            .as_ref()
            .context("API base URL required")?
            .clone();

        info!(
            "Initializing OpenAI-compatible embeddings: model={}, dimensions={}",
            config.model, config.dimensions
        );

        Ok(Self {
            client: Client::new(),
            api_base,
            api_key,
            model: config.model.clone(),
            dimensions: config.dimensions,
        })
    }
}

#[derive(Serialize)]
struct OpenAiEmbeddingRequest {
    input: Vec<String>,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<usize>,
}

#[derive(Deserialize)]
struct OpenAiEmbeddingResponse {
    data: Vec<OpenAiEmbeddingData>,
}

#[derive(Deserialize)]
struct OpenAiEmbeddingData {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbeddingService for OpenAiEmbeddings {
    async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        debug!("Generating embeddings for {} texts", texts.len());

        let request = OpenAiEmbeddingRequest {
            input: texts,
            model: self.model.clone(),
            dimensions: Some(self.dimensions),
        };

        let url = format!("{}/embeddings", self.api_base);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send embedding request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Embedding API error ({}): {}", status, error_text);
        }

        let result: OpenAiEmbeddingResponse = response
            .json()
            .await
            .context("Failed to parse embedding response")?;

        Ok(result.data.into_iter().map(|d| d.embedding).collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}

/// Ollama embedding provider
pub struct OllamaEmbeddings {
    client: Client,
    api_base: String,
    model: String,
    dimensions: usize,
}

impl OllamaEmbeddings {
    pub fn new(config: &EmbeddingConfig) -> Result<Self> {
        let api_base = config
            .api_base
            .as_ref()
            .unwrap_or(&"http://localhost:11434".to_string())
            .clone();

        info!(
            "Initializing Ollama embeddings: model={}, dimensions={}",
            config.model, config.dimensions
        );

        Ok(Self {
            client: Client::new(),
            api_base,
            model: config.model.clone(),
            dimensions: config.dimensions,
        })
    }
}

#[derive(Serialize)]
struct OllamaEmbeddingRequest {
    model: String,
    prompt: String,
}

#[derive(Deserialize)]
struct OllamaEmbeddingResponse {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbeddingService for OllamaEmbeddings {
    async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        debug!(
            "Generating embeddings for {} texts using Ollama",
            texts.len()
        );

        // Ollama doesn't support batch embeddings, so we need to call it for each text
        let mut embeddings = Vec::new();

        for text in texts {
            let request = OllamaEmbeddingRequest {
                model: self.model.clone(),
                prompt: text,
            };

            let url = format!("{}/api/embeddings", self.api_base);

            let response = self
                .client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await
                .context("Failed to send Ollama embedding request")?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                anyhow::bail!("Ollama API error ({}): {}", status, error_text);
            }

            let result: OllamaEmbeddingResponse = response
                .json()
                .await
                .context("Failed to parse Ollama response")?;

            embeddings.push(result.embedding);
        }

        Ok(embeddings)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}

/// Prepare text for embedding by combining title and content
pub fn prepare_block_text(title: &str, content: &str, max_length: usize) -> String {
    let combined = if title.is_empty() {
        content.to_string()
    } else {
        format!("{}\n\n{}", title, content)
    };

    // Truncate if too long (OpenAI has token limits)
    if combined.len() > max_length {
        combined.chars().take(max_length).collect()
    } else {
        combined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepare_block_text() {
        let text = prepare_block_text("Title", "Content here", 1000);
        assert_eq!(text, "Title\n\nContent here");
    }

    #[test]
    fn test_prepare_block_text_truncate() {
        let long_content = "x".repeat(2000);
        let text = prepare_block_text("Title", &long_content, 100);
        assert!(text.len() <= 100);
    }

    #[test]
    fn test_prepare_block_text_no_title() {
        let text = prepare_block_text("", "Content only", 1000);
        assert_eq!(text, "Content only");
    }
}
