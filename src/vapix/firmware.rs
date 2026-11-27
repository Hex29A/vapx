use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::vapix::client::{HttpError, Sent, VapixClient};

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

/// Whether `err` says the camera answers but has not finished booting: the
/// firmware API returns error 500 "The system hasn't finished booting yet."
/// (as a JSON error with HTTP 200, or as an HTTP 500) for up to a minute after
/// a reboot, even while systemready.cgi already says yes. An HTTP 503 in that
/// window means the same. Such an answer is "not ready yet", not a failure.
pub fn is_still_booting(err: &anyhow::Error) -> bool {
    if let Some(e) = err.downcast_ref::<HttpError>() {
        if e.status == 503 {
            return true;
        }
    }
    format!("{:#}", err).to_lowercase().contains("finished booting")
}

/// How often a call that got "not finished booting" is tried again.
pub const BOOT_RETRY_INTERVAL: Duration = if cfg!(test) { Duration::from_millis(50) } else { Duration::from_secs(3) };

/// Run `call` until it stops failing with "not finished booting" or until
/// `deadline`. Any other result, success or error, is returned at once; after
/// the deadline the last "still booting" error is returned. `on_wait` runs
/// before the first retry, so a caller can say once that it is waiting.
pub fn retry_while_booting<T>(
    deadline: Instant,
    mut on_wait: impl FnMut(),
    mut call: impl FnMut() -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let mut waited = false;
    loop {
        match call() {
            Err(err) if is_still_booting(&err) && Instant::now() + BOOT_RETRY_INTERVAL < deadline => {
                if !waited {
                    on_wait();
                    waited = true;
                }
                std::thread::sleep(BOOT_RETRY_INTERVAL);
            }
            result => return result,
        }
    }
}

/// Seconds left before the camera rolls back an uncommitted firmware, from the
/// status call, when the camera reports it.
pub fn time_to_rollback(client: &VapixClient) -> Option<u64> {
    status(client).ok()?.pointer("/data/timeToRollback")?.as_u64()
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
