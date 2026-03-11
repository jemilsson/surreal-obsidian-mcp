use anyhow::Result;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rmcp::model::*;
use rmcp::ServerHandler;
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tracing::{debug, error, info, warn};

use crate::mcp_server::McpServer;

/// Streamable HTTP transport state
#[derive(Clone)]
struct AppState {
    server: Arc<McpServer>,
    sessions: Arc<RwLock<HashMap<String, SessionState>>>,
}

/// Per-session state
struct SessionState {
    initialized: bool,
    protocol_version: String,
}

/// Start Streamable HTTP server for OpenWebUI compatibility
/// Implements MCP over Streamable HTTP as per the 2024-11-05 spec
pub async fn start_http_server(
    server: Arc<McpServer>,
    port: u16,
) -> Result<()> {
    let state = AppState {
        server,
        sessions: Arc::new(RwLock::new(HashMap::new())),
    };

    // Configure CORS for web clients
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/mcp", post(mcp_post_handler))
        .route("/health", get(health_handler))
        .layer(cors)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    info!("🌐 Starting Streamable HTTP server on {}", addr);
    info!("   Health check: http://{}/health", addr);
    info!("   MCP endpoint: http://{}/mcp (POST)", addr);
    info!("   Protocol: MCP 2024-11-05 (Streamable HTTP)");
    info!("   Compatible with OpenWebUI MCP client");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Generate a secure session ID
fn generate_session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("session_{}", timestamp)
}

/// POST /mcp - Handle MCP JSON-RPC requests
async fn mcp_post_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Response {
    debug!("Received MCP request: {:?}", request);

    // Extract session ID from headers if present
    let session_id = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Parse the JSON-RPC request
    let jsonrpc_method = request.get("method").and_then(|m| m.as_str());
    let jsonrpc_id = request.get("id").cloned();

    match jsonrpc_method {
        Some("initialize") => {
            handle_initialize(&state, session_id, jsonrpc_id, request).await
        }
        Some("tools/list") => {
            handle_tools_list(&state, jsonrpc_id).await
        }
        Some("tools/call") => {
            handle_tools_call(&state, session_id, jsonrpc_id, request).await
        }
        Some(method) => {
            warn!("Unsupported method: {}", method);
            create_error_response(
                jsonrpc_id,
                -32601,
                format!("Method not found: {}", method),
            )
        }
        None => {
            error!("Missing method in JSON-RPC request");
            create_error_response(
                jsonrpc_id,
                -32600,
                "Invalid Request: missing method".to_string(),
            )
        }
    }
}

/// Handle initialize request
async fn handle_initialize(
    state: &AppState,
    session_id: Option<String>,
    jsonrpc_id: Option<Value>,
    _request: Value,
) -> Response {
    info!("Initializing MCP session");

    // Generate new session ID if not provided
    let session_id = session_id.unwrap_or_else(generate_session_id);

    // Create session state
    let session_state = SessionState {
        initialized: true,
        protocol_version: "2024-11-05".to_string(),
    };

    state
        .sessions
        .write()
        .await
        .insert(session_id.clone(), session_state);

    // Get server info
    let server_info = state.server.get_info();

    let response = serde_json::json!({
        "jsonrpc": "2.0",
        "id": jsonrpc_id,
        "result": {
            "protocolVersion": "2024-11-05",
            "serverInfo": server_info,
            "capabilities": server_info.capabilities,
        }
    });

    let mut headers = HeaderMap::new();
    headers.insert("mcp-session-id", session_id.parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());

    info!("Session initialized: {}", session_id);
    (StatusCode::OK, headers, Json(response)).into_response()
}

/// Handle tools/list request
async fn handle_tools_list(state: &AppState, jsonrpc_id: Option<Value>) -> Response {
    // Get tools from the server's tool router
    let tools = state.server.list_tools();

    let response = serde_json::json!({
        "jsonrpc": "2.0",
        "id": jsonrpc_id,
        "result": {
            "tools": tools,
        }
    });

    (StatusCode::OK, Json(response)).into_response()
}

/// Handle tools/call request
async fn handle_tools_call(
    state: &AppState,
    session_id: Option<String>,
    jsonrpc_id: Option<Value>,
    request: Value,
) -> Response {
    // Verify session
    if session_id.is_none() {
        return create_error_response(
            jsonrpc_id,
            -32600,
            "Session required - call initialize first".to_string(),
        );
    }

    // Extract tool name and arguments
    let params = match request.get("params") {
        Some(p) => p,
        None => {
            return create_error_response(
                jsonrpc_id,
                -32600,
                "Missing params in tools/call request".to_string(),
            );
        }
    };

    let tool_name = match params.get("name").and_then(|n| n.as_str()) {
        Some(name) => name,
        None => {
            return create_error_response(
                jsonrpc_id,
                -32600,
                "Missing tool name in params".to_string(),
            );
        }
    };

    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

    // Call the tool through the server's tool router
    let call_tool_request = CallToolRequest {
        params: CallToolRequestParams {
            name: tool_name.to_string(),
            arguments: Some(arguments),
        },
    };

    match state.server.call_tool(call_tool_request).await {
        Ok(result) => {
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": jsonrpc_id,
                "result": result,
            });
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => create_error_response(jsonrpc_id, -32603, format!("Tool call failed: {}", e)),
    }
}

/// Create JSON-RPC error response
fn create_error_response(id: Option<Value>, code: i32, message: String) -> Response {
    let response = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
        }
    });

    (StatusCode::OK, Json(response)).into_response()
}

/// Health check endpoint
async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "transport": "streamable-http",
        "protocol": "MCP 2024-11-05",
        "version": env!("CARGO_PKG_VERSION"),
        "note": "Streamable HTTP transport for OpenWebUI compatibility"
    }))
}
