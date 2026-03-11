// HTTP transport using rmcp's built-in Streamable HTTP server

use anyhow::Result;
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager,
    tower::{StreamableHttpServerConfig, StreamableHttpService},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use crate::db::Block;
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

    // Create axum router: REST vault API + search + MCP fallback
    let app = Router::new()
        .route("/", get(|| async { axum::response::Redirect::to("/docs") }))
        .route("/docs", get(docs_handler))
        .route("/vault/{*path}", get(vault_handler))
        .route("/search", get(search_handler).post(search_handler_post))
        .with_state(server)
        .fallback_service(
            tower::ServiceBuilder::new()
                .layer(cors)
                .service(http_service),
        );

    // Bind and serve
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    info!("HTTP server listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(Deserialize)]
struct VaultQuery {
    q: Option<String>,
}

/// GET /vault/{*path}[?query=expression]
///
/// Returns raw markdown for a file in the vault, or the result of an mq query as JSON.
async fn vault_handler(
    State(server): State<Arc<McpServer>>,
    Path(path): Path<String>,
    Query(params): Query<VaultQuery>,
) -> Response {
    let vault_path = server.config().vault.path.clone();
    let file_path = vault_path.join(&path);

    // Security: ensure the resolved path stays within the vault root
    let canonical_vault = match vault_path.canonicalize() {
        Ok(p) => p,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Vault path error").into_response(),
    };
    let canonical_file = match file_path.canonicalize() {
        Ok(p) => p,
        Err(_) => return (StatusCode::NOT_FOUND, format!("Not found: {}", path)).into_response(),
    };
    if !canonical_file.starts_with(&canonical_vault) {
        return (StatusCode::FORBIDDEN, "Path outside vault").into_response();
    }

    // Read raw markdown from disk
    let content = match std::fs::read_to_string(&canonical_file) {
        Ok(c) => c,
        Err(_) => return (StatusCode::NOT_FOUND, format!("Not found: {}", path)).into_response(),
    };

    // If no query, return raw markdown
    let Some(query) = params.q else {
        return (
            [(
                axum::http::header::CONTENT_TYPE,
                "text/markdown; charset=utf-8",
            )],
            content,
        )
            .into_response();
    };

    // Apply mq query directly on the raw markdown content using mq_lang
    let runtime_values = match mq_lang::parse_markdown_input(&content) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to parse markdown: {}", e),
            )
                .into_response()
        }
    };

    let mut engine = mq_lang::DefaultEngine::default();
    match engine.eval(&query, runtime_values.into_iter()) {
        Ok(results) => {
            let markdown: String = results
                .values()
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("\n\n");
            (
                [(
                    axum::http::header::CONTENT_TYPE,
                    "text/markdown; charset=utf-8",
                )],
                markdown,
            )
                .into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, format!("Query error: {}", e)).into_response(),
    }
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default = "default_search_limit")]
    limit: usize,
    #[serde(default)]
    expand: u8,
}

fn default_search_limit() -> usize {
    10
}

#[derive(Serialize)]
struct SearchResult {
    id: String,
    title: String,
    file_path: String,
    content_address: String,
    content_preview: String,
}

impl From<&Block> for SearchResult {
    fn from(b: &Block) -> Self {
        SearchResult {
            id: b.id.clone(),
            title: b.title.clone(),
            file_path: b.file_path.clone(),
            content_address: b.content_address.clone(),
            content_preview: b
                .content
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(200)
                .collect(),
        }
    }
}

/// GET /search?q=text[&limit=N][&expand=N]
async fn search_handler(
    State(server): State<Arc<McpServer>>,
    Query(params): Query<SearchQuery>,
) -> Response {
    match server
        .do_search(&params.q, params.limit, params.expand)
        .await
    {
        Ok((core, expanded)) => {
            let results: Vec<SearchResult> = core.iter().map(SearchResult::from).collect();
            let expanded_results: Vec<SearchResult> =
                expanded.iter().map(SearchResult::from).collect();
            Json(serde_json::json!({
                "query": params.q,
                "results": results,
                "expanded": expanded_results,
            }))
            .into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// POST /search — accepts JSON body `{q, limit, expand}` or plain text/markdown body.
///
/// When posting plain text, use `Content-Type: text/plain` or `text/markdown`.
/// Optionally set `X-Limit: N` header to control result count (default 10).
/// Optionally set `X-Expand: N` header for context expansion depth (default 0).
async fn search_handler_post(
    State(server): State<Arc<McpServer>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let (q, limit, expand) = if content_type.starts_with("application/json") {
        match serde_json::from_slice::<SearchQuery>(&body) {
            Ok(params) => (params.q, params.limit, params.expand),
            Err(e) => {
                return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {e}")).into_response()
            }
        }
    } else {
        // Plain text / markdown body — query is the full body text
        let q = match std::str::from_utf8(&body) {
            Ok(s) => s.trim().to_string(),
            Err(_) => return (StatusCode::BAD_REQUEST, "Body must be valid UTF-8").into_response(),
        };
        if q.is_empty() {
            return (StatusCode::BAD_REQUEST, "Search query is empty").into_response();
        }
        let limit = headers
            .get("x-limit")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(10);
        let expand = headers
            .get("x-expand")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u8>().ok())
            .unwrap_or(0);
        (q, limit, expand)
    };

    match server.do_search(&q, limit, expand).await {
        Ok((core, expanded)) => {
            let results: Vec<SearchResult> = core.iter().map(SearchResult::from).collect();
            let expanded_results: Vec<SearchResult> =
                expanded.iter().map(SearchResult::from).collect();
            Json(serde_json::json!({
                "query": q,
                "results": results,
                "expanded": expanded_results,
            }))
            .into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn docs_handler() -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/markdown; charset=utf-8",
        )],
        r#"# surreal-obsidian-mcp API

## REST API

### GET /docs
This page.

### GET /vault/{path}
Returns the raw markdown of a file in the vault.

    GET /vault/Jonas/hobbies/shooting/Weapon-Licenses.md

Optionally apply an [mq](https://github.com/harehare/mq) query with `?q=`:

    GET /vault/Jonas/Me.md?q=select(.[] | headings)

### GET /search
Search the vault. Returns JSON with `results` and `expanded` arrays.

| Parameter | Default | Description |
|-----------|---------|-------------|
| `q`       | required | Search query |
| `limit`   | 10 | Max results |
| `expand`  | 0 | Graph expansion depth |

    GET /search?q=weapon+licenses&limit=5

### POST /search
Same as GET but accepts a body.

**JSON body** (`Content-Type: application/json`):

    {"q": "weapon licenses", "limit": 5, "expand": 0}

**Plain text body** (any other Content-Type):

    POST /search
    Content-Type: text/plain
    X-Limit: 5
    X-Expand: 0

    weapon licenses

Headers `X-Limit` and `X-Expand` control result count and graph expansion depth.

---

## MCP (Model Context Protocol)

Connect AI assistants via the MCP Streamable HTTP transport at the server root (`/`).

### Tools

| Tool | Description |
|------|-------------|
| `search` | Semantic or keyword search across the vault |
| `get_block` | Read a file or section by path (e.g. `Vault/folder/note.md`) |
| `get_blocks_by_file` | Get all sections of a file |
| `get_all_files` | List all files in the vault |
| `get_children` | Get child sections of a block |
| `create_block` | Create a new note or heading |
| `update_block` | Update an existing block's content |
| `delete_block` | Delete a block |
| `execute_mq_query` | Run an mq query against a block |

### MCP address format

- File: `Vault/folder/note.md`
- Heading: `Vault/folder/note.md#Heading Title`
- With mq query: `Vault/folder/note.md?q=headings`
"#,
    )
}
