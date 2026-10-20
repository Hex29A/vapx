use serde_json::{json, Value};

use crate::vapix::client::{Sent, VapixClient};

/// Get firmware status
pub fn status(client: &VapixClient) -> anyhow::Result<Value> {
    let body = json!({
        "apiVersion": "1.0",
        "method": "status"
    });
    client.post_json("/axis-cgi/firmwaremanagement.cgi", &body)
}

/// The firmware version the camera is running, or `unknown`.
pub fn active_version(client: &VapixClient) -> anyhow::Result<String> {
    let resp = status(client)?;
    Ok(resp
        .pointer("/data/activeFirmwareVersion")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string())
}

/// Upgrade firmware by uploading a .bin file.
/// Returns the JSON response from the camera (contains new firmwareVersion on success).
/// The camera will reboot after a successful upgrade.
#[allow(dead_code)]
pub fn upgrade(
    client: &VapixClient,
    firmware_data: &[u8],
    factory_default: Option<&str>,
    auto_commit: Option<&str>,
    auto_rollback: Option<&str>,
) -> anyhow::Result<Value> {
    let mut params = json!({});
    if let Some(fd) = factory_default {
        params["factoryDefaultMode"] = json!(fd);
    }
    if let Some(ac) = auto_commit {
        params["autoCommit"] = json!(ac);
    }
    if let Some(ar) = auto_rollback {
        params["autoRollback"] = json!(ar);
    }

    let mut json_body = json!({
        "apiVersion": "1.0",
        "method": "upgrade"
    });
    if params.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
        json_body["params"] = params;
    }

    client.post_multipart_firmware(
        "/axis-cgi/firmwaremanagement.cgi",
        &json_body,
        firmware_data,
    )
}

/// Upload and install firmware with a progress bar.
pub fn upgrade_with_progress(
    client: &VapixClient,
    firmware_data: &[u8],
    factory_default: Option<&str>,
    auto_commit: Option<&str>,
    auto_rollback: Option<&str>,
    progress: &indicatif::ProgressBar,
) -> anyhow::Result<Value> {
    let mut params = json!({});
    if let Some(fd) = factory_default {
        params["factoryDefaultMode"] = json!(fd);
    }
    if let Some(ac) = auto_commit {
        params["autoCommit"] = json!(ac);
    }
    if let Some(ar) = auto_rollback {
        params["autoRollback"] = json!(ar);
    }

    let mut json_body = json!({
        "apiVersion": "1.0",
        "method": "upgrade"
    });
    if params.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
        json_body["params"] = params;
    }

    client.post_multipart_firmware_with_progress(
        "/axis-cgi/firmwaremanagement.cgi",
        &json_body,
        firmware_data,
        progress,
    )
}

/// Commit the current firmware (stops auto-rollback timer).
pub fn commit(client: &VapixClient) -> anyhow::Result<Value> {
    let body = json!({
        "apiVersion": "1.0",
        "method": "commit"
    });
    client.post_json("/axis-cgi/firmwaremanagement.cgi", &body)
}

/// Rollback to previously installed firmware.
pub fn rollback(client: &VapixClient) -> anyhow::Result<Value> {
    let body = json!({
        "apiVersion": "1.0",
        "method": "rollback"
    });
    client.post_json("/axis-cgi/firmwaremanagement.cgi", &body)
}

/// Reboot the camera. Sent once, never retried: the camera drops the
/// connection as it goes down, and a retry could reboot it a second time.
pub fn reboot(client: &VapixClient) -> anyhow::Result<Sent> {
    let body = json!({
        "apiVersion": "1.0",
        "method": "reboot"
    });
    client.post_json_once("/axis-cgi/firmwaremanagement.cgi", &body)
}

/// Whether the camera answers a firmware status request right now. One
/// attempt, no retries and no log noise, for polling while it reboots.
pub fn is_up(client: &VapixClient) -> bool {
    let body = json!({
        "apiVersion": "1.0",
        "method": "status"
    });
    matches!(
        client.post_json_once("/axis-cgi/firmwaremanagement.cgi", &body),
        Ok(Sent::Answered)
    )
}

/// Factory default the camera. Sent once, never retried (see `reboot`).
pub fn factory_default(client: &VapixClient, mode: &str) -> anyhow::Result<Sent> {
    let body = json!({
        "apiVersion": "1.0",
        "method": "factoryDefault",
        "params": {
            "factoryDefaultMode": mode
        }
    });
    client.post_json_once("/axis-cgi/firmwaremanagement.cgi", &body)
}
