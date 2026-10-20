use anyhow::bail;
use serde_json::json;
use tracing::debug;

use crate::vapix::client::VapixClient;

/// Fetch I/O port configuration.
/// Tries the modern JSON portmanagement API first, falls back to param.cgi root.IOPort.
pub fn get_ports(client: &VapixClient) -> anyhow::Result<String> {
    // Try legacy param.cgi first (it works on most cameras)
    let params = [("action", "list"), ("group", "root.IOPort")];
    let text = client.get_text("/axis-cgi/param.cgi", &params)?;
    if !text.starts_with("# Error:") {
        return Ok(text);
    }

    debug!("Legacy IOPort params unavailable, trying portmanagement API");

    // Fall back to modern JSON API
    match client.post_json(
        "/axis-cgi/io/portmanagement.cgi",
        &json!({
            "apiVersion": "1.0",
            "method": "getPorts",
        }),
    ) {
        Ok(resp) => {
            // Format JSON response as key=value text for consistency
            Ok(serde_json::to_string_pretty(&resp)?)
        }
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("404") {
                bail!("I/O port configuration not available on this camera (neither param.cgi root.IOPort nor portmanagement.cgi). Use 'vapx discover' to check supported APIs.")
            } else {
                Err(e)
            }
        }
    }
}

/// Update I/O port parameters. Each entry is (param_name, value).
pub fn set_params(client: &VapixClient, assignments: &[(&str, &str)]) -> anyhow::Result<String> {
    let mut params: Vec<(&str, &str)> = vec![("action", "update")];
    params.extend_from_slice(assignments);
    let text = client.get_text("/axis-cgi/param.cgi", &params)?;
    if text.starts_with("# Error:") || text.contains("Error:") {
        bail!("param.cgi: {}", text.trim());
    }
    Ok(text)
}

/// Trigger an I/O output port to active or inactive state.
/// Uses the legacy io/port.cgi endpoint (works on all firmware).
/// `port_index` is 0-based (maps to 1-based port number in the API).
/// `active` true = activate (close), false = deactivate (open).
pub fn trigger_port(client: &VapixClient, port_index: u8, active: bool) -> anyhow::Result<String> {
    let port_num = port_index + 1;
    // The action value uses / for active and \ for inactive.
    // We pre-encode to avoid over-encoding of : and / by the query builder.
    let state = if active { "%2F" } else { "%5C" };
    let path = format!("/axis-cgi/io/port.cgi?action={}:{}", port_num, state);
    let text = client.get_text(&path, &[])?;
    if text.contains("Error") {
        bail!("io/port.cgi: {}", text.trim());
    }
    Ok(text)
}

/// I/O ports as a list of `{port, direction, name, state, ...}`.
///
/// Uses the `portmanagement.cgi` JSON API and, on cameras without it, the
/// `param.cgi` + `io/port.cgi` pair. `state` is `open` or `closed`.
pub fn get_port_list(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    let modern = client.post_json(
        "/axis-cgi/io/portmanagement.cgi",
        &json!({"apiVersion": "1.0", "method": "getPorts"}),
    );
    if let Ok(resp) = modern {
        let mut data = super::data_of(resp);
        return Ok(data.get_mut("items").map(serde_json::Value::take).unwrap_or_else(|| json!([])));
    }
    let params = client.get_text("/axis-cgi/param.cgi", &[("action", "list"), ("group", "IOPort")])?;
    let count = params.lines().filter(|l| l.contains(".Direction=")).count();
    if count == 0 {
        return Ok(json!([]));
    }
    let ids: Vec<String> = (1..=count).map(|i| i.to_string()).collect();
    let states = client.get_text("/axis-cgi/io/port.cgi", &[("checkactive", &ids.join(","))])?;
    Ok(parse_legacy_ports(&params, &states))
}

fn parse_legacy_ports(param_text: &str, state_text: &str) -> serde_json::Value {
    let mut order: Vec<String> = Vec::new();
    let mut ports: std::collections::HashMap<String, serde_json::Map<String, serde_json::Value>> =
        std::collections::HashMap::new();
    for line in param_text.lines() {
        let Some((key, value)) = line.trim().split_once('=') else { continue };
        let parts: Vec<&str> = key.split('.').collect();
        if parts.len() < 4 || parts[1] != "IOPort" {
            continue;
        }
        let id = parts[2].to_string();
        if !ports.contains_key(&id) {
            order.push(id.clone());
            let mut m = serde_json::Map::new();
            m.insert("port".into(), json!((order.len() - 1).to_string()));
            ports.insert(id.clone(), m);
        }
        match parts[3] {
            "Direction" => {
                ports.get_mut(&id).unwrap().insert("direction".into(), json!(value.to_lowercase()));
            }
            "Usage" => {
                ports.get_mut(&id).unwrap().insert("name".into(), json!(value));
            }
            _ => {}
        }
    }
    let mut list: Vec<serde_json::Map<String, serde_json::Value>> =
        order.iter().map(|id| ports.remove(id).unwrap()).collect();
    for line in state_text.lines() {
        let Some((key, value)) = line.trim().split_once('=') else { continue };
        if let Some(n) = key.strip_prefix("port").and_then(|n| n.parse::<usize>().ok()) {
            if let Some(port) = n.checked_sub(1).and_then(|i| list.get_mut(i)) {
                let state = if value.trim() == "active" { "closed" } else { "open" };
                port.insert("state".into(), json!(state));
            }
        }
    }
    serde_json::Value::Array(list.into_iter().map(serde_json::Value::Object).collect())
}
