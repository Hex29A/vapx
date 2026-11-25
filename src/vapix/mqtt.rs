use crate::vapix::client::VapixClient;
use crate::vapix::discover;
use serde_json::{json, Value};

/// The newest `mqtt-client` API version vapx's requests are written for.
const MQTT_CLIENT_API: &str = "1.6";

/// The `apiVersion` to send to `client.cgi`: what the device reports in API
/// discovery, but never newer than [`MQTT_CLIENT_API`]. AXIS OS 10.12 reports
/// 1.5 and refuses 1.6 with error 2100. Without an answer from discovery,
/// fall back to the newest known version.
fn client_api_version(client: &VapixClient) -> String {
    pick_version(discover::api_version(client, "mqtt-client").as_deref())
}

fn pick_version(reported: Option<&str>) -> String {
    match reported {
        Some(v) if discover::version_key(v) < discover::version_key(MQTT_CLIENT_API) => v.to_string(),
        _ => MQTT_CLIENT_API.to_string(),
    }
}

fn client_call(client: &VapixClient, method: &str, params: Option<&Value>) -> anyhow::Result<Value> {
    let mut body = json!({
        "apiVersion": client_api_version(client),
        "method": method,
    });
    if let Some(p) = params {
        body["params"] = p.clone();
    }
    client.post_json("/axis-cgi/mqtt/client.cgi", &body)
}

/// Get MQTT client status and configuration.
pub fn get_client_status(client: &VapixClient) -> anyhow::Result<Value> {
    client_call(client, "getClientStatus", None)
}

/// Configure the MQTT client.
pub fn configure_client(client: &VapixClient, params: &Value) -> anyhow::Result<Value> {
    client_call(client, "configureClient", Some(params))
}

/// Activate (enable) the MQTT client.
pub fn activate_client(client: &VapixClient) -> anyhow::Result<Value> {
    client_call(client, "activateClient", None)
}

/// Deactivate (disable) the MQTT client.
pub fn deactivate_client(client: &VapixClient) -> anyhow::Result<Value> {
    client_call(client, "deactivateClient", None)
}

/// Get event publication configuration from the MQTT event bridge.
pub fn get_event_config(client: &VapixClient) -> anyhow::Result<Value> {
    client.post_json("/axis-cgi/mqtt/event.cgi", &json!({
        "apiVersion": "1.2",
        "method": "getEventPublicationConfig",
    }))
}

/// Set event publication configuration on the MQTT event bridge.
///
/// `config` is the full event publication config object (the same shape that
/// [`get_event_config`] returns under `data.eventPublicationConfig`): it carries
/// `eventFilterList` plus fields like `topicPrefix` / `appendEventTopic`. Callers
/// should read the current config, modify only the field(s) they care about
/// (typically `eventFilterList`), and pass the whole object back so unrelated
/// fields are preserved. Axis only enables publication once the filter list is
/// non-empty.
///
/// Note the request/response asymmetry: `getEventPublicationConfig` wraps the
/// config in an `eventPublicationConfig` object, but `configureEventPublication`
/// expects those fields (notably `eventFilterList`) directly under `params`.
pub fn set_event_config(client: &VapixClient, config: &Value) -> anyhow::Result<Value> {
    client.post_json("/axis-cgi/mqtt/event.cgi", &json!({
        "apiVersion": "1.2",
        "method": "configureEventPublication",
        "params": config,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vapix::testserver::{Reply, TestServer};

    #[test]
    fn picks_the_device_version_but_never_a_newer_one() {
        assert_eq!(pick_version(Some("1.5")), "1.5");
        assert_eq!(pick_version(Some("1.6")), "1.6");
        assert_eq!(pick_version(Some("1.7")), "1.6");
        assert_eq!(pick_version(None), "1.6");
    }

    /// A camera on AXIS OS 10.12: discovery lists mqtt-client 1.5 and the
    /// client API refuses anything else with error 2100.
    fn camera_with_mqtt_1_5() -> TestServer {
        TestServer::start(|req| {
            if req.contains("apidiscovery.cgi") {
                Reply::Json(200, r#"{"apiVersion":"1.0","data":{"apiList":[{"id":"mqtt-client","version":"1.5"}]}}"#)
            } else if req.contains(r#""apiVersion":"1.5""#) {
                Reply::Json(200, r#"{"apiVersion":"1.5","data":{"status":{"state":"inactive"}}}"#)
            } else {
                Reply::Json(200, r#"{"apiVersion":"1.5","error":{"code":2100,"message":"API version not supported: 'apiVersion' value (1.6) not supported"}}"#)
            }
        })
    }

    #[test]
    fn status_and_writes_use_the_version_the_camera_reports() {
        let srv = camera_with_mqtt_1_5();
        let c = srv.client();
        assert!(get_client_status(&c).is_ok());
        assert!(activate_client(&c).is_ok());
        assert!(deactivate_client(&c).is_ok());
        assert!(configure_client(&c, &json!({"server":{"host":"192.0.2.1"}})).is_ok());
        let sent: Vec<String> = srv.requests().into_iter().filter(|r| r.contains("client.cgi")).collect();
        assert_eq!(sent.len(), 4);
        assert!(sent.iter().all(|r| r.contains(r#""apiVersion":"1.5""#)));
    }

    #[test]
    fn falls_back_to_1_6_when_discovery_is_unavailable() {
        let srv = TestServer::start(|req| {
            if req.contains("apidiscovery.cgi") {
                Reply::Json(404, "{}")
            } else {
                Reply::Json(200, r#"{"data":{}}"#)
            }
        });
        get_client_status(&srv.client()).unwrap();
        let sent = srv.requests().into_iter().find(|r| r.contains("client.cgi")).unwrap();
        assert!(sent.contains(r#""apiVersion":"1.6""#));
    }
}
