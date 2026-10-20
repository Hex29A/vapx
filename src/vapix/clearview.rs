//! Clear View (`clearviewcontrol.cgi`).

use serde_json::{json, Value};

use crate::vapix::client::VapixClient;

/// The camera's Clear View services.
pub fn get_service_info(client: &VapixClient) -> anyhow::Result<Value> {
    let resp = client.post_json(
        "/axis-cgi/clearviewcontrol.cgi",
        &json!({"apiVersion": "1.0", "method": "getServiceInfo", "params": {}}),
    )?;
    let mut data = super::data_of(resp);
    Ok(data.get_mut("serviceInfo").map(Value::take).unwrap_or(data))
}
