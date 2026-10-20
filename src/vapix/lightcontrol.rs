//! Light control (`lightcontrol.cgi`): IR, white and indicator lights.

use serde_json::{json, Value};

use crate::vapix::client::VapixClient;

pub fn get_light_information(client: &VapixClient) -> anyhow::Result<Value> {
    let resp = client.post_json(
        "/axis-cgi/lightcontrol.cgi",
        &json!({"apiVersion": "1.0", "method": "getLightInformation", "params": {}}),
    )?;
    let mut data = super::data_of(resp);
    Ok(data.get_mut("items").map(Value::take).unwrap_or(data))
}
