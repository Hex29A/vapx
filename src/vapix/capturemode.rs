//! Capture modes (`capturemode.cgi`).

use serde_json::{json, Value};

use crate::vapix::client::VapixClient;

pub fn get_capture_modes(client: &VapixClient) -> anyhow::Result<Value> {
    let resp = client.post_json(
        "/axis-cgi/capturemode.cgi",
        &json!({"apiVersion": "1.0", "method": "getCaptureModes"}),
    )?;
    Ok(super::data_of(resp))
}

/// Switch `channel` to a capture mode. The camera has to reboot to apply it.
pub fn set_capture_mode(client: &VapixClient, channel: i64, capture_mode_id: i64) -> anyhow::Result<()> {
    client.post_json(
        "/axis-cgi/capturemode.cgi",
        &json!({"apiVersion": "1.0", "method": "setCaptureMode",
                "params": {"channel": channel, "captureModeId": capture_mode_id}}),
    )?;
    Ok(())
}
