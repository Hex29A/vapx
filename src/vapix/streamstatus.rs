use anyhow::bail;
use tracing::debug;

use crate::vapix::client::VapixClient;

/// Get stream status. The streamstatus.cgi requires specific transport;
/// we fall back to reading stream parameters from param.cgi.
pub fn get_stream_status(client: &VapixClient) -> anyhow::Result<String> {
    // Try the modern streamstatus.cgi first
    let body = serde_json::json!({
        "apiVersion": "1.0",
        "method": "getStreamStatus",
    });
    match client.post_json("/axis-cgi/streamstatus.cgi", &body) {
        Ok(resp) => return Ok(serde_json::to_string(&resp)?),
        Err(e) => {
            let msg = format!("{}", e);
            debug!("streamstatus.cgi failed: {}, falling back to stream params", msg);
        }
    }

    // Fallback: read stream parameters from param.cgi
    let text = client.get_text(
        "/axis-cgi/param.cgi",
        &[("action", "list"), ("group", "root.Properties.Streaming")],
    )?;
    if text.starts_with("# Error:") {
        // Try Image stream params instead
        let text2 = client.get_text(
            "/axis-cgi/param.cgi",
            &[("action", "list"), ("group", "root.Image.I0.Stream")],
        )?;
        if text2.starts_with("# Error:") {
            bail!("Stream status not available on this camera");
        }
        return Ok(text2);
    }
    Ok(text)
}
