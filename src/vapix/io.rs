use anyhow::bail;
use serde_json::json;
use tracing::debug;

use crate::vapix::client::VapixClient;

/// Fetch I/O port configuration.
/// Tries the modern JSON portmanagement API first, falls back to param.cgi root.IOPort.
pub fn get_ports(client: &VapixClient) -> anyhow::Result<String> {
    // Try legacy param.cgi first (it works on most cameras)
    let params = [("action", "list"), ("group", "root.IOPort")];
    let text = client.get_text("/axis-cgi/param.cgi", &params)?;
    if !text.starts_with("# Error:") {
        return Ok(text);
    }

    debug!("Legacy IOPort params unavailable, trying portmanagement API");

    // Fall back to modern JSON API
    match client.post_json(
        "/axis-cgi/io/portmanagement.cgi",
        &json!({
            "apiVersion": "1.0",
            "method": "getPorts",
        }),
    ) {
        Ok(resp) => {
            // Format JSON response as key=value text for consistency
            Ok(serde_json::to_string_pretty(&resp)?)
        }
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("404") {
                bail!("I/O port configuration not available on this camera (neither param.cgi root.IOPort nor portmanagement.cgi)")
            } else {
                Err(e)
            }
        }
    }
}

/// Update I/O port parameters. Each entry is (param_name, value).
pub fn set_params(client: &VapixClient, assignments: &[(&str, &str)]) -> anyhow::Result<String> {
    let mut params: Vec<(&str, &str)> = vec![("action", "update")];
    params.extend_from_slice(assignments);
    let text = client.get_text("/axis-cgi/param.cgi", &params)?;
    if text.starts_with("# Error:") || text.contains("Error:") {
        bail!("param.cgi: {}", text.trim());
    }
    Ok(text)
}
