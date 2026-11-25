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

/// The version a device reports for one API in API discovery, e.g. `"1.5"`
/// for `mqtt-client`. `None` when discovery fails or the API is not listed.
pub fn api_version(client: &VapixClient, api_id: &str) -> Option<String> {
    let resp = get_api_list(client).ok()?;
    version_in_list(&resp, api_id)
}

/// Whether API discovery lists `api_id`. `None` when discovery itself gave no
/// usable answer, so the caller should try the API rather than assume it is absent.
pub fn api_listed(client: &VapixClient, api_id: &str) -> Option<bool> {
    let resp = get_api_list(client).ok()?;
    let list = resp.pointer("/data/apiList")?.as_array()?;
    Some(list.iter().any(|a| a.get("id").and_then(Value::as_str) == Some(api_id)))
}

/// Pick `api_id`'s version out of a `getApiList` answer.
pub fn version_in_list(resp: &Value, api_id: &str) -> Option<String> {
    resp.pointer("/data/apiList")?
        .as_array()?
        .iter()
        .find(|a| a.get("id").and_then(Value::as_str) == Some(api_id))
        .and_then(|a| a.get("version"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Compare two `major.minor` versions. Anything unparseable sorts lowest.
pub fn version_key(v: &str) -> (u32, u32) {
    let mut it = v.split('.').map(|p| p.trim().parse::<u32>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_read_from_the_api_list() {
        let list = json!({"data":{"apiList":[
            {"id":"mqtt-client","version":"1.5"},
            {"id":"other","version":"2.0"}]}});
        assert_eq!(version_in_list(&list, "mqtt-client").as_deref(), Some("1.5"));
        assert_eq!(version_in_list(&list, "streamstatus"), None);
        assert_eq!(version_in_list(&json!({}), "mqtt-client"), None);
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(version_key("1.10") > version_key("1.6"));
        assert!(version_key("1.5") < version_key("1.6"));
        assert_eq!(version_key("garbage"), (0, 0));
    }
}
