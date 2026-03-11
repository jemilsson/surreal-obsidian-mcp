use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub vault: VaultConfig,
    pub database: DatabaseConfig,
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub reranking: RerankingConfig,
    #[serde(default)]
    pub sync: SyncConfig,
    #[serde(default)]
    pub graph: GraphConfig,
    #[serde(default)]
    pub transport: TransportConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VaultConfig {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddingConfig {
    pub provider: EmbeddingProvider,
    pub model: String,
    pub dimensions: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_cache: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EmbeddingProvider {
    OpenAi,
    OpenAiCompatible,
    Ollama,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RerankingConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<RerankingProvider>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_cache: Option<PathBuf>,
    #[serde(default = "default_top_n")]
    pub top_n: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RerankingProvider {
    Cohere,
    Jina,
    Ollama,
    #[cfg(feature = "embedded")]
    Embedded,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SyncConfig {
    #[serde(default = "default_true")]
    pub watch_for_changes: bool,
    #[serde(default = "default_true")]
    pub initial_indexing: bool,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GraphConfig {
    #[serde(default = "default_true")]
    pub extract_links: bool,
    #[serde(default = "default_true")]
    pub extract_backlinks: bool,
    #[serde(default = "default_true")]
    pub extract_tags: bool,
    #[serde(default = "default_true")]
    pub extract_mentions: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransportConfig {
    #[serde(default = "default_transport_type")]
    pub transport_type: TransportType,
    /// Port for HTTP/SSE transport (default: 3000)
    #[serde(default = "default_http_port")]
    pub http_port: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TransportType {
    /// Standard I/O transport (for Claude Desktop)
    Stdio,
    /// HTTP/SSE transport (for OpenWebUI)
    Http,
}

// Default value functions
fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_top_n() -> usize {
    20
}

fn default_batch_size() -> usize {
    100
}

fn default_transport_type() -> TransportType {
    TransportType::Stdio
}

fn default_http_port() -> u16 {
    3000
}

impl Default for RerankingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: None,
            model: None,
            api_key: None,
            api_base: None,
            model_cache: None,
            top_n: default_top_n(),
        }
    }
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            watch_for_changes: true,
            initial_indexing: true,
            batch_size: default_batch_size(),
        }
    }
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            extract_links: true,
            extract_backlinks: true,
            extract_tags: true,
            extract_mentions: true,
        }
    }
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            transport_type: default_transport_type(),
            http_port: default_http_port(),
        }
    }
}

impl Config {
    /// Load configuration from a JSON file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file: {}", path.as_ref().display()))?;

        let config: Config = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.as_ref().display()))?;

        config.validate()?;

        Ok(config)
    }

    /// Validate the configuration
    fn validate(&self) -> Result<()> {
        // Validate vault path exists
        if !self.vault.path.exists() {
            anyhow::bail!("Vault path does not exist: {}", self.vault.path.display());
        }

        if !self.vault.path.is_dir() {
            anyhow::bail!(
                "Vault path is not a directory: {}",
                self.vault.path.display()
            );
        }

        // Validate embedding config
        match self.embedding.provider {
            EmbeddingProvider::OpenAi
            | EmbeddingProvider::OpenAiCompatible
            | EmbeddingProvider::Ollama => {
                if self.embedding.api_base.is_none() {
                    anyhow::bail!(
                        "api_base is required for {:?} provider",
                        self.embedding.provider
                    );
                }
            }
        }

        // Validate reranking config if enabled
        if self.reranking.enabled {
            if self.reranking.provider.is_none() {
                anyhow::bail!("Reranking provider must be specified when reranking is enabled");
            }
            if self.reranking.model.is_none() {
                anyhow::bail!("Reranking model must be specified when reranking is enabled");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_example_config() {
        let config_str = r#"{
  "vault": {
    "path": "/tmp/test-vault"
  },
  "database": {
    "path": "./test.db"
  },
  "embedding": {
    "provider": "open-ai",
    "model": "text-embedding-3-small",
    "dimensions": 1536,
    "api_key": "sk-test",
    "api_base": "https://api.openai.com/v1"
  },
  "reranking": {
    "enabled": false
  },
  "sync": {
    "watch_for_changes": true,
    "initial_indexing": true,
    "batch_size": 100
  },
  "graph": {
    "extract_links": true,
    "extract_backlinks": true,
    "extract_tags": true,
    "extract_mentions": true
  }
}"#;

        let config: Config = serde_json::from_str(config_str).unwrap();
        assert_eq!(config.vault.path, PathBuf::from("/tmp/test-vault"));
        assert_eq!(config.embedding.provider, EmbeddingProvider::OpenAi);
        assert_eq!(config.embedding.model, "text-embedding-3-small");
        assert_eq!(config.embedding.dimensions, 1536);
        assert!(!config.reranking.enabled);
    }
}
