use serde_json::{json, Value};

use crate::vapix::client::VapixClient;

/// Get all device properties via basicdeviceinfo.cgi
pub fn get_all_properties(client: &VapixClient) -> anyhow::Result<Value> {
    let body = json!({
        "apiVersion": "1.0",
        "method": "getAllProperties"
    });
    let resp = client.post_json("/axis-cgi/basicdeviceinfo.cgi", &body)?;
    Ok(resp)
}

/// Get specific device properties
pub fn get_properties(client: &VapixClient, properties: &[&str]) -> anyhow::Result<Value> {
    let body = json!({
        "apiVersion": "1.0",
        "method": "getProperties",
        "params": {
            "propertyList": properties
        }
    });
    let resp = client.post_json("/axis-cgi/basicdeviceinfo.cgi", &body)?;
    Ok(resp)
}
