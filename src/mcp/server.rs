//! The MCP server: lists the registry's tools and runs them on a blocking
//! thread (the VAPIX client is synchronous).

use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use tracing::{debug, warn};

use super::tools::{Args, Out, Registry};

#[derive(Clone)]
pub struct VapxMcp {
    registry: Arc<Registry>,
}

impl VapxMcp {
    pub fn new() -> Self {
        Self { registry: Arc::new(Registry::new()) }
    }
}

/// Run a tool and turn failures into an error result with `Error: …` text,
/// as the Python server did.
pub fn run_tool(registry: &Registry, name: &str, args: Args) -> Vec<ContentBlock> {
    let result = match registry.handler(name) {
        Some(handler) => registry.validate(name, &args).and_then(|()| handler(&args)),
        None => Err(anyhow::anyhow!("Unknown tool: {}", name)),
    };
    match result {
        Ok(out) => out.into_iter().map(to_block).collect(),
        Err(e) => vec![ContentBlock::text(format!("Error: {}", e))],
    }
}

fn to_block(out: Out) -> ContentBlock {
    match out {
        Out::Text(t) => ContentBlock::text(t),
        Out::Image(data, mime) => ContentBlock::image(data, mime),
    }
}

impl ServerHandler for VapxMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("vapx", env!("CARGO_PKG_VERSION")))
            .with_instructions("Axis camera tools over VAPIX. Start with list_cameras to get camera IDs.")
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.registry.list()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let name = request.name.to_string();
        debug!("tool call: {}", name);
        let args = Args(request.arguments.unwrap_or_default());
        let registry = self.registry.clone();
        let tool = name.clone();
        let content = tokio::task::spawn_blocking(move || run_tool(&registry, &tool, args))
            .await
            .map_err(|e| {
                warn!("tool {} panicked: {}", name, e);
                ErrorData::internal_error(format!("tool {} failed unexpectedly", name), None)
            })?;
        let is_error = matches!(content.first(), Some(ContentBlock::Text(t)) if t.text.starts_with("Error: "));
        let result = if is_error { CallToolResult::error(content) } else { CallToolResult::success(content) };
        Ok(result.into())
    }
}
