//! `vapx mcp` — an MCP server over the CLI's VAPIX library.
//!
//! Tool names, parameters and descriptions are those of the Python
//! `vapx-mcp` server (`tools.json`), so existing clients keep working.

mod auth;
mod camera;
mod server;
mod tools;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{bail, Context};
use clap::{Args, ValueEnum};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{transport::stdio, ServiceExt};
use tokio_util::sync::CancellationToken;

const TOKEN_ENV: &str = "VAPX_MCP_TOKEN";

#[derive(Clone, Copy, ValueEnum)]
pub enum Transport {
    /// Standard input and output (for clients that start the server themselves)
    Stdio,
    /// Streamable HTTP at /mcp (needs --bind and a bearer token)
    Http,
}

#[derive(Args)]
pub struct McpCmd {
    /// Transport to serve on
    #[arg(long, value_enum, default_value = "stdio")]
    pub transport: Transport,

    /// Address to listen on for --transport http, e.g. 127.0.0.1:8080 (no default)
    #[arg(long)]
    pub bind: Option<SocketAddr>,

    /// Extra Host header value to accept besides the bind address and localhost
    /// (repeatable, or comma-separated in the environment variable), e.g. a DNS
    /// name the server is reached by
    #[arg(long = "allowed-host", env = "VAPX_MCP_ALLOWED_HOSTS", value_delimiter = ',')]
    pub allowed_hosts: Vec<String>,

    /// Serve HTTP without a bearer token. Only for a trusted loopback setup.
    #[arg(long)]
    pub insecure_no_auth: bool,
}

impl McpCmd {
    pub fn run(self) -> anyhow::Result<()> {
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
        match self.transport {
            Transport::Stdio => rt.block_on(serve_stdio()),
            Transport::Http => {
                let bind = self.bind.context("--transport http needs --bind <addr:port>")?;
                let token = http_token(self.insecure_no_auth, std::env::var(TOKEN_ENV).ok())?;
                rt.block_on(serve_http(bind, token, self.allowed_hosts))
            }
        }
    }
}

/// The bearer token to require, or `None` when auth was switched off on purpose.
/// Refuses to continue without either.
fn http_token(insecure_no_auth: bool, env_value: Option<String>) -> anyhow::Result<Option<String>> {
    match env_value.filter(|t| !t.is_empty()) {
        Some(t) => Ok(Some(t)),
        None if insecure_no_auth => Ok(None),
        None => bail!(
            "{} is not set. Set it to a secret token, or pass --insecure-no-auth to serve without one.",
            TOKEN_ENV
        ),
    }
}

async fn serve_stdio() -> anyhow::Result<()> {
    let running = server::VapxMcp::new().serve(stdio()).await.context("MCP initialisation failed")?;
    running.waiting().await?;
    Ok(())
}

async fn serve_http(bind: SocketAddr, token: Option<String>, extra_hosts: Vec<String>) -> anyhow::Result<()> {
    let ct = CancellationToken::new();
    let mut hosts: Vec<String> = ["localhost", "127.0.0.1", "::1"].iter().map(|s| s.to_string()).collect();
    hosts.push(bind.ip().to_string());
    hosts.push(bind.to_string());
    hosts.extend(extra_hosts);

    let config = StreamableHttpServerConfig::default()
        .with_allowed_hosts(hosts)
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_cancellation_token(ct.child_token());
    let service = StreamableHttpService::new(
        || Ok(server::VapxMcp::new()),
        Arc::new(LocalSessionManager::default()),
        config,
    );

    let mut router = axum::Router::new().nest_service("/mcp", service);
    match token {
        Some(token) => {
            router = router.layer(axum::middleware::from_fn_with_state(Arc::new(token), auth::require_token));
        }
        None => eprintln!("WARNING: serving without authentication (--insecure-no-auth)"),
    }

    let listener = tokio::net::TcpListener::bind(bind).await.with_context(|| format!("cannot listen on {bind}"))?;
    eprintln!("vapx mcp listening on http://{bind}/mcp");
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            ct.cancel();
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::http_token;

    #[test]
    fn http_refuses_to_start_without_a_token() {
        let err = http_token(false, None).unwrap_err().to_string();
        assert!(err.contains("VAPX_MCP_TOKEN"), "{err}");
        assert!(http_token(false, Some(String::new())).is_err());
    }

    #[test]
    fn http_uses_the_token_or_the_explicit_opt_out() {
        assert_eq!(http_token(false, Some("s3cret".into())).unwrap().as_deref(), Some("s3cret"));
        assert_eq!(http_token(true, None).unwrap(), None);
        // A token wins over the opt-out: auth stays on when one is set.
        assert_eq!(http_token(true, Some("s3cret".into())).unwrap().as_deref(), Some("s3cret"));
    }
}
