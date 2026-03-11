// HTTP transport temporarily disabled pending rmcp 0.16 API integration
//
// The rmcp 0.16 update changed the API significantly, requiring RequestContext
// which cannot be easily created outside the rmcp transport layer.
//
// This needs proper refactoring to either:
// 1. Use rmcp's built-in HTTP transport (if available in future versions)
// 2. Properly integrate with rmcp's Service layer
// 3. Downgrade to rmcp 0.15 if HTTP transport is critical
//
// For now, use stdio transport which works correctly with rmcp 0.16.

use anyhow::Result;
use std::sync::Arc;

use crate::mcp_server::McpServer;

/// Start HTTP server - currently disabled
pub async fn start_http_server(_server: Arc<McpServer>, _port: u16) -> Result<()> {
    anyhow::bail!(
        "HTTP transport is not yet compatible with rmcp 0.16. \
         Please use stdio transport for now. \
         Configure transport_type: 'stdio' in your config.json"
    )
}
