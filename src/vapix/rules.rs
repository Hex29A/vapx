use crate::vapix::client::VapixClient;
use serde_json::json;
use tracing::debug;

/// Attempt an action rule API call, providing a clear error on 404.
fn action_request(client: &VapixClient, body: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    match client.post_json("/axis-cgi/action/action.cgi", body) {
        Ok(resp) => Ok(resp),
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("404") {
                debug!("Action rule API returned 404");
                anyhow::bail!(
                    "Action rule API not available on this camera. \
                     The action/event API on this firmware uses SOAP (/vapix/services) \
                     which is not yet supported. Use 'vapx discover' to check supported APIs."
                )
            } else {
                Err(e)
            }
        }
    }
}

/// List all action rules.
pub fn list_rules(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    action_request(
        client,
        &json!({
            "apiVersion": "1.0",
            "method": "listRules",
        }),
    )
}

/// Get a specific rule by ID.
pub fn get_rule(client: &VapixClient, rule_id: &str) -> anyhow::Result<serde_json::Value> {
    action_request(
        client,
        &json!({
            "apiVersion": "1.0",
            "method": "getRuleInfo",
            "params": {
                "ruleID": rule_id,
            },
        }),
    )
}

/// Remove a rule by ID.
pub fn remove_rule(client: &VapixClient, rule_id: &str) -> anyhow::Result<serde_json::Value> {
    action_request(
        client,
        &json!({
            "apiVersion": "1.0",
            "method": "removeRule",
            "params": {
                "ruleID": rule_id,
            },
        }),
    )
}

/// Enable or disable a rule.
pub fn set_rule_enabled(client: &VapixClient, rule_id: &str, enabled: bool) -> anyhow::Result<serde_json::Value> {
    action_request(
        client,
        &json!({
            "apiVersion": "1.0",
            "method": "setRuleEnabled",
            "params": {
                "ruleID": rule_id,
                "enabled": enabled,
            },
        }),
    )
}

/// List available action templates (trigger types).
pub fn list_templates(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    action_request(
        client,
        &json!({
            "apiVersion": "1.0",
            "method": "listActionTemplates",
        }),
    )
}

/// List available recipient templates (action types).
pub fn list_recipients(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    action_request(
        client,
        &json!({
            "apiVersion": "1.0",
            "method": "listRecipientTemplates",
        }),
    )
}
