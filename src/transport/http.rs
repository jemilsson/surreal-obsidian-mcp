// HTTP transport using rmcp's built-in Streamable HTTP server

use anyhow::Result;
use axum::Router;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager,
    tower::{StreamableHttpServerConfig, StreamableHttpService},
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use crate::mcp_server::McpServer;

/// Start HTTP server using rmcp's built-in Streamable HTTP support
pub async fn start_http_server(server: Arc<McpServer>, port: u16) -> Result<()> {
    info!("Starting HTTP server on port {}", port);

    // Create session manager for stateful connections
    let session_manager = Arc::new(LocalSessionManager::default());

    // Configure the HTTP server
    let config = StreamableHttpServerConfig {
        sse_keep_alive: Some(std::time::Duration::from_secs(15)),
        sse_retry: Some(std::time::Duration::from_secs(3)),
        stateful_mode: true,
        cancellation_token: CancellationToken::new(),
    };

    // Create the streamable HTTP service
    // The service_factory creates a new server instance for each request/session
    let http_service = StreamableHttpService::new(
        {
            let server = server.clone();
            move || Ok(server.clone())
        },
        session_manager,
        config,
    );

    // Configure CORS for web clients
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Create axum router with the rmcp HTTP service as a tower service
    let app = Router::new()
        .fallback_service(tower::ServiceBuilder::new()
            .layer(cors)
            .service(http_service));

    // Bind and serve
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    info!("HTTP server listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
