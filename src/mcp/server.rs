//! The MCP server: lists the registry's tools and runs them on a blocking
//! thread (the VAPIX client is synchronous).

use std::sync::Arc;
use std::time::Duration;

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

/// How long one tool call may run before the client gets an error back.
/// `VAPX_MCP_TOOL_TIMEOUT` (seconds) overrides it. The default leaves room for
/// a firmware upload (300 s on the camera side) and a reboot wait.
const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 600;

fn tool_deadline() -> Duration {
    let secs = std::env::var("VAPX_MCP_TOOL_TIMEOUT")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_TOOL_TIMEOUT_SECS);
    Duration::from_secs(secs)
}

/// How a bounded blocking call ended.
enum Bounded<T> {
    Done(T),
    Panicked,
    TimedOut,
}

/// Run `f` on a blocking thread and wait at most `limit` for it. A thread
/// cannot be cancelled, so on a timeout it runs on in the background; the
/// caller just stops waiting.
async fn run_bounded<T: Send + 'static>(limit: Duration, f: impl FnOnce() -> T + Send + 'static) -> Bounded<T> {
    match tokio::time::timeout(limit, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(value)) => Bounded::Done(value),
        Ok(Err(_)) => Bounded::Panicked,
        Err(_) => Bounded::TimedOut,
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
        let limit = tool_deadline();
        let content = match run_bounded(limit, move || run_tool(&registry, &tool, args)).await {
            Bounded::Done(content) => content,
            Bounded::Panicked => {
                warn!("tool {} panicked", name);
                return Err(ErrorData::internal_error(format!("tool {} failed unexpectedly", name), None));
            }
            Bounded::TimedOut => {
                warn!("tool {} did not finish within {} s", name, limit.as_secs());
                vec![ContentBlock::text(format!(
                    "Error: {} did not finish within {} s. The camera may still have carried it out; check its state before trying again.",
                    name,
                    limit.as_secs()
                ))]
            }
        };
        let is_error = matches!(content.first(), Some(ContentBlock::Text(t)) if t.text.starts_with("Error: "));
        let result = if is_error { CallToolResult::error(content) } else { CallToolResult::success(content) };
        Ok(result.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_tool_that_never_returns_ends_in_a_timeout() {
        let started = std::time::Instant::now();
        let out = run_bounded(Duration::from_millis(100), || std::thread::sleep(Duration::from_secs(3))).await;
        assert!(matches!(out, Bounded::TimedOut));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn a_tool_within_the_limit_returns_its_result() {
        let out = run_bounded(Duration::from_secs(5), || 7).await;
        assert!(matches!(out, Bounded::Done(7)));
    }

    #[tokio::test]
    async fn a_panicking_tool_is_reported_not_propagated() {
        let out = run_bounded(Duration::from_secs(5), || -> u8 { panic!("boom") }).await;
        assert!(matches!(out, Bounded::Panicked));
    }

    #[test]
    fn the_deadline_has_a_default_and_an_override() {
        // Tests share the environment; keep all variable handling in this one test.
        std::env::remove_var("VAPX_MCP_TOOL_TIMEOUT");
        assert_eq!(tool_deadline(), Duration::from_secs(DEFAULT_TOOL_TIMEOUT_SECS));
        std::env::set_var("VAPX_MCP_TOOL_TIMEOUT", "30");
        assert_eq!(tool_deadline(), Duration::from_secs(30));
        std::env::set_var("VAPX_MCP_TOOL_TIMEOUT", "0");
        assert_eq!(tool_deadline(), Duration::from_secs(DEFAULT_TOOL_TIMEOUT_SECS));
        std::env::set_var("VAPX_MCP_TOOL_TIMEOUT", "soon");
        assert_eq!(tool_deadline(), Duration::from_secs(DEFAULT_TOOL_TIMEOUT_SECS));
        std::env::remove_var("VAPX_MCP_TOOL_TIMEOUT");
    }
}
