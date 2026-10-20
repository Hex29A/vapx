//! Siren and light (`siren_and_light.cgi`).

use serde_json::{json, Value};

use crate::vapix::client::VapixClient;

pub fn get_status(client: &VapixClient) -> anyhow::Result<Value> {
    let resp = client.post_json(
        "/axis-cgi/siren_and_light.cgi",
        &json!({"apiVersion": "1.0", "method": "getStatus"}),
    )?;
    Ok(super::data_of(resp))
}

/// Start the siren and/or light. `params` is the `start` parameter object
/// (`{"profile": ...}` or `{"siren": {...}, "light": {...}}`). Returns the
/// camera's answer.
pub fn start(client: &VapixClient, params: &Value) -> anyhow::Result<Value> {
    let resp = client.post_json(
        "/axis-cgi/siren_and_light.cgi",
        &json!({"apiVersion": "1.0", "method": "start", "params": params}),
    )?;
    Ok(super::data_of(resp))
}

/// Stop the siren and the light.
pub fn stop(client: &VapixClient) -> anyhow::Result<()> {
    client.post_json(
        "/axis-cgi/siren_and_light.cgi",
        &json!({"apiVersion": "1.0", "method": "stop", "params": {"all": ["siren", "light"]}}),
    )?;
    Ok(())
}
