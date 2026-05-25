use crate::vapix::client::VapixClient;
use serde_json::{json, Value};

/// Run device self-test.
/// Note: This API may require the camera to be in "preview mode" (factory setup).
pub fn run_self_test(client: &VapixClient) -> anyhow::Result<Value> {
    match client.post_json("/axis-cgi/deviceselftest.cgi", &json!({
        "apiVersion": "1.0",
        "method": "runSelfTest",
    })) {
        Ok(resp) => Ok(resp),
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("2105") || msg.contains("preview mode") || msg.contains("401") {
                anyhow::bail!(
                    "Device self-test requires the camera to be in preview mode \
                     (initial factory setup) or admin credentials. \
                     This is normal for deployed cameras."
                )
            } else if msg.contains("404") {
                anyhow::bail!(
                    "Device self-test API not available on this camera. \
                     Requires AXIS OS 12.x or newer."
                )
            } else {
                Err(e)
            }
        }
    }
}

/// Get self-test result (if previously run).
#[allow(dead_code)]
pub fn get_self_test_result(client: &VapixClient) -> anyhow::Result<Value> {
    match client.post_json("/axis-cgi/deviceselftest.cgi", &json!({
        "apiVersion": "1.0",
        "method": "getSelfTestResult",
    })) {
        Ok(resp) => Ok(resp),
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("2105") || msg.contains("preview mode") || msg.contains("401") {
                anyhow::bail!(
                    "Device self-test requires the camera to be in preview mode \
                     (initial factory setup) or admin credentials. \
                     This is normal for deployed cameras."
                )
            } else if msg.contains("404") {
                anyhow::bail!(
                    "Device self-test API not available on this camera. \
                     Requires AXIS OS 12.x or newer."
                )
            } else {
                Err(e)
            }
        }
    }
}
