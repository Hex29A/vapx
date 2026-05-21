use serde_json::{json, Value};

use crate::vapix::client::VapixClient;

/// Get firmware status
pub fn status(client: &VapixClient) -> anyhow::Result<Value> {
    let body = json!({
        "apiVersion": "1.0",
        "method": "status"
    });
    client.post_json("/axis-cgi/firmwaremanagement.cgi", &body)
}
