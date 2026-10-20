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

/// Start a Clear View service (wiper or speed-dry), optionally for `duration` seconds.
pub fn start(client: &VapixClient, service_id: i64, duration: Option<i64>) -> anyhow::Result<()> {
    let mut params = json!({"id": service_id});
    if let Some(d) = duration {
        params["duration"] = json!(d);
    }
    client.post_json(
        "/axis-cgi/clearviewcontrol.cgi",
        &json!({"apiVersion": "1.0", "method": "start", "params": params}),
    )?;
    Ok(())
}

pub fn stop(client: &VapixClient, service_id: i64) -> anyhow::Result<()> {
    client.post_json(
        "/axis-cgi/clearviewcontrol.cgi",
        &json!({"apiVersion": "1.0", "method": "stop", "params": {"id": service_id}}),
    )?;
    Ok(())
}
