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
