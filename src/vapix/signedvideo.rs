use crate::vapix::client::VapixClient;
use serde_json::{json, Value};
use tracing::debug;

/// Post to signedvideo.cgi, returning Ok(resp) or falling back to param.cgi
/// on 404 for read operations, or re-raising the error for write operations.
fn post_signedvideo(
    client: &VapixClient,
    body: &Value,
) -> Result<Value, SignedVideoError> {
    match client.post_json("/axis-cgi/signedvideo.cgi", body) {
        Ok(resp) => Ok(resp),
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("404") {
                debug!("signedvideo.cgi returned 404");
                Err(SignedVideoError::NotFound)
            } else {
                Err(SignedVideoError::Other(e))
            }
        }
    }
}

enum SignedVideoError {
    NotFound,
    Other(anyhow::Error),
}

/// Get signed video status.
/// Tries `signedvideo.cgi` first; on 404 falls back to reading signed video
/// parameters via `param.cgi` (root.SignedVideo / root.Properties.SignedVideo).
pub fn get_status(client: &VapixClient) -> anyhow::Result<Value> {
    match post_signedvideo(client, &json!({
        "apiVersion": "1.0",
        "method": "getStatus",
    })) {
        Ok(resp) => Ok(resp),
        Err(SignedVideoError::NotFound) => get_status_from_params(client),
        Err(SignedVideoError::Other(e)) => Err(e),
    }
}

/// Fallback: read signed video configuration from param.cgi groups.
fn get_status_from_params(client: &VapixClient) -> anyhow::Result<Value> {
    debug!("Falling back to param.cgi for signed video status");
    let mut params = serde_json::Map::new();
    let mut found = false;

    // Try root.SignedVideo
    if let Ok(text) = client.get_text(
        "/axis-cgi/param.cgi",
        &[("action", "list"), ("group", "root.SignedVideo")],
    ) {
        if !text.starts_with("# Error:") {
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    // Strip "root.SignedVideo." prefix for cleaner output
                    let key = k.strip_prefix("root.SignedVideo.").unwrap_or(k);
                    params.insert(key.to_string(), json!(v));
                    found = true;
                }
            }
        }
    }

    // Also try root.Properties.API.SignedVideo for capability info
    if let Ok(text) = client.get_text(
        "/axis-cgi/param.cgi",
        &[("action", "list"), ("group", "root.Properties.API.SignedVideo")],
    ) {
        if !text.starts_with("# Error:") {
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    let key = k.strip_prefix("root.Properties.API.").unwrap_or(k);
                    params.insert(key.to_string(), json!(v));
                    found = true;
                }
            }
        }
    }

    if !found {
        anyhow::bail!(
            "Signed video API not available on this camera \
             (signedvideo.cgi returned 404 and no SignedVideo parameters found). \
             Use 'vapx discover' to check supported APIs."
        );
    }

    Ok(json!({
        "data": {
            "signedVideo": params,
            "source": "param_cgi_fallback",
        }
    }))
}

/// Enable signed video.
/// Tries `signedvideo.cgi` first; on 404 falls back to setting the parameter
/// via `param.cgi`.
pub fn enable(client: &VapixClient) -> anyhow::Result<Value> {
    match post_signedvideo(client, &json!({
        "apiVersion": "1.0",
        "method": "setEnabled",
        "params": {"enabled": true},
    })) {
        Ok(resp) => Ok(resp),
        Err(SignedVideoError::NotFound) => set_enabled_via_params(client, true),
        Err(SignedVideoError::Other(e)) => Err(e),
    }
}

/// Disable signed video.
/// Tries `signedvideo.cgi` first; on 404 falls back to setting the parameter
/// via `param.cgi`.
pub fn disable(client: &VapixClient) -> anyhow::Result<Value> {
    match post_signedvideo(client, &json!({
        "apiVersion": "1.0",
        "method": "setEnabled",
        "params": {"enabled": false},
    })) {
        Ok(resp) => Ok(resp),
        Err(SignedVideoError::NotFound) => set_enabled_via_params(client, false),
        Err(SignedVideoError::Other(e)) => Err(e),
    }
}

/// Fallback: set signed video enabled/disabled via param.cgi.
fn set_enabled_via_params(client: &VapixClient, enabled: bool) -> anyhow::Result<Value> {
    debug!("Falling back to param.cgi to set signed video enabled={}", enabled);
    let val = if enabled { "yes" } else { "no" };
    let text = client.get_text(
        "/axis-cgi/param.cgi",
        &[
            ("action", "update"),
            ("root.SignedVideo.Enabled", val),
        ],
    )?;
    if text.starts_with("# Error:") || text.contains("Error") {
        anyhow::bail!(
            "Failed to set signed video via param.cgi: {}. \
             The signedvideo.cgi endpoint is also not available (404).",
            text.trim()
        );
    }
    Ok(json!({
        "data": {
            "enabled": enabled,
            "source": "param_cgi_fallback",
        }
    }))
}
