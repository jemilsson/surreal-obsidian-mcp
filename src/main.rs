use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

use surreal_obsidian_mcp::config::{Config, TransportType};
use surreal_obsidian_mcp::db::Database;
use surreal_obsidian_mcp::mcp_server::McpServer;
use surreal_obsidian_mcp::sync::Synchronizer;
use surreal_obsidian_mcp::transport::http;

#[derive(Parser, Debug)]
#[command(
    name = "surreal-obsidian-mcp",
    about = "MCP server for indexing Obsidian vaults into SurrealDB",
    version
)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "config.json")]
    config: PathBuf,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize tracing
    let log_level = if args.debug {
        Level::DEBUG
    } else {
        Level::INFO
    };
    // When running under systemd, journald adds its own timestamps — skip ours
    let in_systemd = std::env::var("JOURNAL_STREAM").is_ok();
    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_target(false)
        .with_ansi(!in_systemd)
        .with_timer(tracing_subscriber::fmt::time::SystemTime)
        .compact();
    if in_systemd {
        tracing::subscriber::set_global_default(subscriber.without_time().finish())?;
    } else {
        tracing::subscriber::set_global_default(subscriber.finish())?;
    }

    info!("🦀 Surreal Obsidian MCP starting...");
    info!("📝 Config file: {}", args.config.display());

    // Load configuration
    let config = Config::load(&args.config)?;
    info!("✅ Configuration loaded");
    info!("   Vault: {}", config.vault.path.display());
    info!("   Database: {}", config.database.path.display());
    info!("   Embedding provider: {:?}", config.embedding.provider);
    info!(
        "   Reranking: {}",
        if config.reranking.enabled {
            "enabled"
        } else {
            "disabled"
        }
    );

    // Initialize SurrealDB
    let db = Database::new(&config.database.path).await?;
    info!("✅ Database initialized");

    // Wrap in Arc<RwLock<>> for sharing between synchronizer and MCP server
    let db = Arc::new(RwLock::new(db));
    let config = Arc::new(config);

    // Create synchronizer
    let sync = Synchronizer::new(db.clone(), config.clone())?;

    // Perform initial indexing
    info!("✅ Starting initial vault indexing...");
    sync.initial_index().await?;

    // Spawn file watcher in background task
    if config.sync.watch_for_changes {
        info!("✅ Starting file watcher...");
        tokio::spawn(async move {
            if let Err(e) = sync.run().await {
                error!("File watcher error: {}", e);
            }
        });
    }

    // Start MCP server with appropriate transport
    info!("✅ Starting MCP server...");
    let server = McpServer::new(db.clone(), config.clone());

    match config.transport.transport_type {
        TransportType::Stdio => {
            info!("   Transport: stdio (for Claude Desktop)");
            server.run().await?;
        }
        TransportType::Http => {
            info!("   Transport: HTTP/SSE (for OpenWebUI)");
            info!("   Port: {}", config.transport.http_port);
            http::start_http_server(Arc::new(server), config.transport.http_port).await?;
        }
    }

    info!("👋 Shutting down...");

    Ok(())
}
