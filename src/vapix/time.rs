use anyhow::bail;

use crate::vapix::client::VapixClient;

/// Fetch time/NTP configuration parameters (root.Time group).
pub fn get_config(client: &VapixClient) -> anyhow::Result<String> {
    let params = [("action", "list"), ("group", "root.Time")];
    let text = client.get_text("/axis-cgi/param.cgi", &params)?;
    if text.starts_with("# Error:") {
        bail!("param.cgi: {}", text.trim());
    }
    Ok(text)
}

/// Update time/NTP parameters. Each entry is (param_name, value).
pub fn set_params(client: &VapixClient, assignments: &[(&str, &str)]) -> anyhow::Result<String> {
    let mut params: Vec<(&str, &str)> = vec![("action", "update")];
    params.extend_from_slice(assignments);
    let text = client.get_text("/axis-cgi/param.cgi", &params)?;
    if text.starts_with("# Error:") || text.contains("Error:") {
        bail!("param.cgi: {}", text.trim());
    }
    Ok(text)
}

/// Date, time and time zone as the camera reports them (`time.cgi`).
pub fn get_date_time_info(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    let resp = client.post_json(
        "/axis-cgi/time.cgi",
        &serde_json::json!({"apiVersion": "1.0", "method": "getDateTimeInfo"}),
    )?;
    Ok(super::data_of(resp))
}

/// NTP client state and servers (`ntp.cgi`).
pub fn get_ntp_info(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    let resp = client.post_json(
        "/axis-cgi/ntp.cgi",
        &serde_json::json!({"apiVersion": "1.0", "method": "getNTPInfo"}),
    )?;
    Ok(super::data_of(resp))
}

/// Set the time zone (IANA name, e.g. `Europe/Stockholm`).
pub fn set_timezone(client: &VapixClient, timezone: &str) -> anyhow::Result<()> {
    client.post_json(
        "/axis-cgi/time.cgi",
        &serde_json::json!({"apiVersion": "1.0", "method": "setTimeZone", "params": {"timeZone": timezone}}),
    )?;
    Ok(())
}

/// Configure the NTP client. Only the given settings change.
pub fn set_ntp_config(
    client: &VapixClient,
    enabled: Option<bool>,
    servers_source: Option<&str>,
    static_servers: Option<&[String]>,
) -> anyhow::Result<()> {
    let mut params = serde_json::Map::new();
    if let Some(v) = enabled {
        params.insert("enabled".into(), serde_json::json!(v));
    }
    if let Some(v) = servers_source {
        params.insert("serversSource".into(), serde_json::json!(v));
    }
    if let Some(v) = static_servers {
        params.insert("staticServers".into(), serde_json::json!(v));
    }
    client.post_json(
        "/axis-cgi/ntp.cgi",
        &serde_json::json!({"apiVersion": "1.0", "method": "setNTPClientConfiguration", "params": params}),
    )?;
    Ok(())
}
