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
