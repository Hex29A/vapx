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
