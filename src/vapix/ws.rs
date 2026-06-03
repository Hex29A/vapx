//! Shared WebSocket helpers for VAPIX data-stream endpoints
//! (events, streamstatus, nexus, …).

use anyhow::{Context, Result};
use tracing::debug;

use crate::config::credentials::Credentials;
use crate::vapix::client::VapixClient;

/// Fetch a single-use WebSocket session token via `/axis-cgi/wssession.cgi`.
/// The token is short-lived; callers should connect immediately.
pub fn get_ws_session(client: &VapixClient) -> Result<String> {
    let text = client
        .get_text("/axis-cgi/wssession.cgi", &[])
        .context("Failed to fetch WebSocket session token")?;
    let token = text.trim().to_string();
    if token.is_empty() {
        anyhow::bail!("wssession.cgi returned an empty token");
    }
    debug!("Got WS session token");
    Ok(token)
}

/// Build a `ws://` (or `wss://` when HTTPS is enabled) URL for
/// `/vapix/ws-data-stream` with the given source(s) and token.
pub fn build_ws_url(creds: &Credentials, host: &str, token: &str, sources: &str) -> String {
    let scheme = if creds.https { "wss" } else { "ws" };
    format!(
        "{}://{}:{}/vapix/ws-data-stream?wssession={}&sources={}",
        scheme, host, creds.port, token, sources
    )
}

/// Build a Nexus video-stream URL (sources=video).
pub fn build_nexus_url(creds: &Credentials, host: &str, token: &str) -> String {
    build_ws_url(creds, host, token, "video")
}
