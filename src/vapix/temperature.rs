use anyhow::bail;

use crate::vapix::client::VapixClient;

/// Fetch temperature sensor data from temperaturecontrol.cgi.
/// Returns the raw key=value text response.
pub fn get_sensors(client: &VapixClient) -> anyhow::Result<String> {
    let resp = client.get_text(
        "/axis-cgi/temperaturecontrol.cgi",
        &[("method", "getSensorList")],
    );
    match resp {
        Ok(text) => {
            if text.starts_with("# Error:") || text.contains("Error:") {
                bail!("temperaturecontrol.cgi: {}", text.trim());
            }
            Ok(text)
        }
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("404") {
                bail!(
                    "Temperature sensor API not available on this camera. \
                     Requires AXIS OS 10.x or newer with temperature sensors."
                )
            } else {
                Err(e)
            }
        }
    }
}
