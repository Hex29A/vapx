use crate::vapix::client::VapixClient;
use serde_json::json;

/// List all action rules.
pub fn list_rules(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    client.post_json(
        "/axis-cgi/action/action.cgi",
        &json!({
            "apiVersion": "1.0",
            "method": "listRules",
        }),
    )
}

/// Get a specific rule by ID.
pub fn get_rule(client: &VapixClient, rule_id: &str) -> anyhow::Result<serde_json::Value> {
    client.post_json(
        "/axis-cgi/action/action.cgi",
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
    client.post_json(
        "/axis-cgi/action/action.cgi",
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
    client.post_json(
        "/axis-cgi/action/action.cgi",
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
    client.post_json(
        "/axis-cgi/action/action.cgi",
        &json!({
            "apiVersion": "1.0",
            "method": "listActionTemplates",
        }),
    )
}

/// List available recipient templates (action types).
pub fn list_recipients(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    client.post_json(
        "/axis-cgi/action/action.cgi",
        &json!({
            "apiVersion": "1.0",
            "method": "listRecipientTemplates",
        }),
    )
}
