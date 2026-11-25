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

/// The model name (`ProdNbr`, e.g. "M2035-LE") in a `getAllProperties` or
/// `getProperties` reply, with or without the `data.propertyList` wrapping.
pub fn model_of(reply: &Value) -> Option<&str> {
    let data = reply.get("data").unwrap_or(reply);
    let props = data.get("propertyList").unwrap_or(data);
    ["ProdNbr", "ProdShortName"].iter().find_map(|k| props.get(*k).and_then(|v| v.as_str())).filter(|m| !m.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_is_the_product_number() {
        let wrapped = json!({"data": {"propertyList": {"ProdNbr": "M2035-LE", "ProdShortName": "AXIS M2035-LE"}}});
        assert_eq!(model_of(&wrapped), Some("M2035-LE"));
        assert_eq!(model_of(&json!({"ProdNbr": "Q1615"})), Some("Q1615"));
        assert_eq!(model_of(&json!({"ProdShortName": "AXIS Q1615"})), Some("AXIS Q1615"));
        assert_eq!(model_of(&json!({"Model": "nope"})), None);
    }
}
