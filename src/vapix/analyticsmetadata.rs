//! Analytics metadata producers (`analyticsmetadataconfig.cgi`).

use serde_json::{json, Value};

use crate::vapix::client::VapixClient;

pub fn list_producers(client: &VapixClient) -> anyhow::Result<Value> {
    let resp = client.post_json(
        "/axis-cgi/analyticsmetadataconfig.cgi",
        &json!({"apiVersion": "1.0", "method": "listProducers"}),
    )?;
    let mut data = super::data_of(resp);
    Ok(data.get_mut("producers").map(Value::take).unwrap_or_else(|| json!([])))
}

/// Enable or disable producers; `producers` entries carry name and channels.
pub fn set_enabled_producers(client: &VapixClient, producers: &Value) -> anyhow::Result<()> {
    client.post_json(
        "/axis-cgi/analyticsmetadataconfig.cgi",
        &json!({"apiVersion": "1.0", "method": "setEnabledProducers", "params": {"producers": producers}}),
    )?;
    Ok(())
}
