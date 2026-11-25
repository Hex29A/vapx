//! Video motion detection (VMD4, `/local/vmd/control.cgi`).

use serde_json::{json, Value};

use crate::vapix::client::VapixClient;

/// What to tell the user when the camera has no VMD: cameras with a recent
/// AXIS OS run AXIS Object Analytics (an ACAP) instead.
pub const NOT_AVAILABLE: &str = "Video motion detection (VMD) is not available on this camera. \
    Newer cameras use AXIS Object Analytics instead: check 'vapx acap list' (or the MCP tools \
    list_applications and list_analytics_producers).";

pub fn get_configuration(client: &VapixClient) -> anyhow::Result<Value> {
    let resp = client
        .post_json("/local/vmd/control.cgi", &json!({"apiVersion": "1.3", "method": "getConfiguration"}))
        .map_err(|e| if crate::vapix::client::is_not_supported(&e) { anyhow::anyhow!(NOT_AVAILABLE) } else { e })?;
    Ok(super::data_of(resp))
}

/// Replace the motion detection configuration with `cameras` and `profiles`,
/// as returned by [`get_configuration`].
pub fn set_configuration(client: &VapixClient, cameras: &Value, profiles: &Value) -> anyhow::Result<()> {
    client.post_json(
        "/local/vmd/control.cgi",
        &json!({"apiVersion": "1.3", "method": "setConfiguration",
                "params": {"cameras": cameras, "profiles": profiles}}),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vapix::testserver::{Reply, TestServer};

    #[test]
    fn a_camera_without_vmd_points_to_object_analytics() {
        let srv = TestServer::start(|_| Reply::Text(404, "Not Found"));
        let err = get_configuration(&srv.client()).unwrap_err().to_string();
        assert!(err.contains("AXIS Object Analytics"), "{err}");
    }
}
