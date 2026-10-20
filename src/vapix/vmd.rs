//! Video motion detection (VMD4, `/local/vmd/control.cgi`).

use serde_json::{json, Value};

use crate::vapix::client::VapixClient;

pub fn get_configuration(client: &VapixClient) -> anyhow::Result<Value> {
    let resp = client.post_json(
        "/local/vmd/control.cgi",
        &json!({"apiVersion": "1.3", "method": "getConfiguration"}),
    )?;
    Ok(super::data_of(resp))
}
