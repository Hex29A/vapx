use crate::vapix::client::VapixClient;
use serde_json::{json, Value};

/// Get MQTT client status and configuration.
pub fn get_client_status(client: &VapixClient) -> anyhow::Result<Value> {
    client.post_json("/axis-cgi/mqtt/client.cgi", &json!({
        "apiVersion": "1.6",
        "method": "getClientStatus",
    }))
}

/// Configure the MQTT client.
pub fn configure_client(client: &VapixClient, params: &Value) -> anyhow::Result<Value> {
    client.post_json("/axis-cgi/mqtt/client.cgi", &json!({
        "apiVersion": "1.6",
        "method": "configureClient",
        "params": params,
    }))
}

/// Activate (enable) the MQTT client.
pub fn activate_client(client: &VapixClient) -> anyhow::Result<Value> {
    client.post_json("/axis-cgi/mqtt/client.cgi", &json!({
        "apiVersion": "1.6",
        "method": "activateClient",
    }))
}

/// Deactivate (disable) the MQTT client.
pub fn deactivate_client(client: &VapixClient) -> anyhow::Result<Value> {
    client.post_json("/axis-cgi/mqtt/client.cgi", &json!({
        "apiVersion": "1.6",
        "method": "deactivateClient",
    }))
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
/// `config` is the full `eventPublicationConfig` object. Callers should read the
/// current config via [`get_event_config`], modify only the field(s) they care
/// about (typically `eventFilterList`), and pass the whole object back so that
/// unrelated fields are preserved. Axis only enables event publication once the
/// filter list is non-empty.
///
/// The VAPIX method is `configureEventPublication` (the read counterpart is
/// `getEventPublicationConfig`).
pub fn set_event_config(client: &VapixClient, config: &Value) -> anyhow::Result<Value> {
    client.post_json("/axis-cgi/mqtt/event.cgi", &json!({
        "apiVersion": "1.2",
        "method": "configureEventPublication",
        "params": {
            "eventPublicationConfig": config,
        },
    }))
}
