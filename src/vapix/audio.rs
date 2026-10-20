//! Audio devices (`audiodevicecontrol.cgi`).

use serde_json::{json, Value};

use crate::vapix::client::VapixClient;

/// Current settings of every audio device.
pub fn get_devices_settings(client: &VapixClient) -> anyhow::Result<Value> {
    let resp = client.post_json(
        "/axis-cgi/audiodevicecontrol.cgi",
        &json!({"apiVersion": "1.0", "method": "getDevicesSettings"}),
    )?;
    Ok(super::data_of(resp))
}

/// Update audio device settings; `devices` is the array from [`get_devices_settings`].
pub fn set_devices_settings(client: &VapixClient, devices: &Value) -> anyhow::Result<()> {
    client.post_json(
        "/axis-cgi/audiodevicecontrol.cgi",
        &json!({"apiVersion": "1.0", "method": "setDevicesSettings", "params": {"devices": devices}}),
    )?;
    Ok(())
}
