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
                     which is only supported for 'rule list'. Use 'vapx discover' to check supported APIs."
                )
            } else {
                Err(e)
            }
        }
    }
}

/// List all action rules: the JSON API first, and on a camera that has none
/// (404) the SOAP action service (`/vapix/services`, `GetActionRules`).
pub fn list_rules(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    match client.post_json("/axis-cgi/action/action.cgi", &json!({"apiVersion": "1.0", "method": "listRules"})) {
        Ok(resp) => Ok(resp),
        Err(e) if crate::vapix::client::is_not_supported(&e) => {
            debug!("Action rule JSON API is not available, trying SOAP");
            let xml = client.post_soap("/vapix/services", GET_ACTION_RULES)?;
            Ok(json!({"data": {"rules": parse_action_rules(&xml)?, "source": "soap"}}))
        }
        Err(e) => Err(e),
    }
}

const GET_ACTION_RULES: &str = r#"<?xml version="1.0" encoding="UTF-8"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope"><SOAP-ENV:Body><aa:GetActionRules xmlns:aa="http://www.axis.com/vapix/ws/action1"/></SOAP-ENV:Body></SOAP-ENV:Envelope>"#;

/// The rules in a `GetActionRulesResponse`: id, name, enabled and the id of
/// the action each triggers. A SOAP fault is an error.
fn parse_action_rules(xml: &str) -> anyhow::Result<Vec<serde_json::Value>> {
    use anyhow::Context;
    let doc = roxmltree::Document::parse(xml).context("Action rule reply is not XML")?;
    if let Some(fault) = doc.descendants().find(|n| n.has_tag_name("Fault")) {
        let reason = fault.descendants().find(|n| n.has_tag_name("Text")).and_then(|n| n.text()).unwrap_or("unknown");
        anyhow::bail!("Action rule service answered with a SOAP fault: {}", reason.trim());
    }
    let text_of = |rule: roxmltree::Node, name: &str| -> Option<String> {
        rule.children().find(|c| c.has_tag_name(name)).and_then(|c| c.text()).map(|t| t.trim().to_string())
    };
    Ok(doc
        .descendants()
        .filter(|n| n.has_tag_name("ActionRule"))
        .map(|rule| {
            json!({
                "id": text_of(rule, "RuleID"),
                "name": text_of(rule, "Name"),
                "enabled": text_of(rule, "Enabled").map(|e| e == "true"),
                "primaryAction": text_of(rule, "PrimaryAction"),
            })
        })
        .collect())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vapix::testserver::{Reply, TestServer};

    const REPLY: &str = r#"<?xml version="1.0"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope"><SOAP-ENV:Body><aa:GetActionRulesResponse xmlns:aa="http://www.axis.com/vapix/ws/action1"><aa:ActionRules><aa:ActionRule><aa:RuleID>1</aa:RuleID><aa:Name>Motion to relay</aa:Name><aa:Enabled>true</aa:Enabled><aa:PrimaryAction>3</aa:PrimaryAction></aa:ActionRule><aa:ActionRule><aa:RuleID>2</aa:RuleID><aa:Name>Old</aa:Name><aa:Enabled>false</aa:Enabled><aa:PrimaryAction>4</aa:PrimaryAction></aa:ActionRule></aa:ActionRules></aa:GetActionRulesResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>"#;

    #[test]
    fn parses_the_rules_of_a_soap_reply() {
        let rules = parse_action_rules(REPLY).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0]["id"], "1");
        assert_eq!(rules[0]["name"], "Motion to relay");
        assert_eq!(rules[0]["enabled"], true);
        assert_eq!(rules[1]["enabled"], false);
        assert!(parse_action_rules("nope").is_err());
    }

    #[test]
    fn a_soap_fault_is_an_error() {
        let fault = r#"<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope"><s:Body><s:Fault><s:Reason><s:Text xml:lang="en">Not allowed</s:Text></s:Reason></s:Fault></s:Body></s:Envelope>"#;
        let err = parse_action_rules(fault).unwrap_err().to_string();
        assert!(err.contains("Not allowed"), "{err}");
    }

    #[test]
    fn list_falls_back_to_soap_when_the_json_api_is_missing() {
        let srv = TestServer::start(|req| {
            if req.contains("/vapix/services") && req.contains("GetActionRules") {
                Reply::Text(200, REPLY)
            } else {
                Reply::Text(404, "Not Found")
            }
        });
        let v = list_rules(&srv.client()).unwrap();
        assert_eq!(v["data"]["source"], "soap");
        assert_eq!(v["data"]["rules"][1]["name"], "Old");
    }
}
