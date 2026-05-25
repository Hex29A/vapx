use crate::vapix::client::VapixClient;
use serde_json::{json, Value};
use tracing::debug;

/// Get signed video status.
pub fn get_status(client: &VapixClient) -> anyhow::Result<Value> {
    match client.post_json("/axis-cgi/signedvideo.cgi", &json!({
        "apiVersion": "1.0",
        "method": "getStatus",
    })) {
        Ok(resp) => Ok(resp),
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("404") {
                debug!("signedvideo.cgi returned 404");
                anyhow::bail!(
                    "Signed video API not available on this camera. \
                     Use 'vapx discover' to check supported APIs."
                )
            } else {
                Err(e)
            }
        }
    }
}

/// Enable signed video.
pub fn enable(client: &VapixClient) -> anyhow::Result<Value> {
    match client.post_json("/axis-cgi/signedvideo.cgi", &json!({
        "apiVersion": "1.0",
        "method": "setEnabled",
        "params": {"enabled": true},
    })) {
        Ok(resp) => Ok(resp),
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("404") {
                anyhow::bail!(
                    "Signed video API not available on this camera. \
                     Use 'vapx discover' to check supported APIs."
                )
            } else {
                Err(e)
            }
        }
    }
}

/// Disable signed video.
pub fn disable(client: &VapixClient) -> anyhow::Result<Value> {
    match client.post_json("/axis-cgi/signedvideo.cgi", &json!({
        "apiVersion": "1.0",
        "method": "setEnabled",
        "params": {"enabled": false},
    })) {
        Ok(resp) => Ok(resp),
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("404") {
                anyhow::bail!(
                    "Signed video API not available on this camera. \
                     Use 'vapx discover' to check supported APIs."
                )
            } else {
                Err(e)
            }
        }
    }
}
