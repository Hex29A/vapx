use serde_json::{json, Value};

use crate::vapix::client::VapixClient;

/// Get list of all supported APIs on the device.
pub fn get_api_list(client: &VapixClient) -> anyhow::Result<Value> {
    let body = json!({
        "method": "getApiList",
        "apiVersion": "1.0"
    });
    client.post_json("/axis-cgi/apidiscovery.cgi", &body)
}
