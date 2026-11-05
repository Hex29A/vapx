//! O3C, One-Click Cloud Connection: the mode (`root.RemoteService.Enabled`), where the
//! camera connects to (`root.RemoteService.ServerList`), and the Owner Authentication
//! Key (`oak.cgi`) that proves ownership towards the O3C dispatcher.
//!
//! The OAK is a credential: it is returned to the caller and never logged here.

use anyhow::bail;
use serde_json::{json, Value};

use crate::vapix::client::VapixClient;
use crate::vapix::{device, params};

const GROUP: &str = "root.RemoteService";
const ENABLED: &str = "root.RemoteService.Enabled";
const OAK: &str = "/axis-cgi/oak.cgi";

/// When the camera connects to the cloud.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Always (`yes`).
    Always,
    /// Only after the control button is pressed (`oneclick`).
    OneClick,
    /// Never (`no`).
    Off,
}

impl Mode {
    pub fn param_value(self) -> &'static str {
        match self {
            Mode::Always => "yes",
            Mode::OneClick => "oneclick",
            Mode::Off => "no",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Always => "always",
            Mode::OneClick => "oneclick",
            Mode::Off => "off",
        }
    }

    pub fn from_param(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "yes" => Some(Mode::Always),
            "oneclick" => Some(Mode::OneClick),
            "no" => Some(Mode::Off),
            _ => None,
        }
    }
}

/// Is a server list still the Axis dispatcher (the camera has not been handed to a
/// service), or has it been assigned? Returns the distinct service domains when assigned.
fn servers(list: &str) -> Value {
    let hosts: Vec<&str> = list
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.rsplit_once(':').map_or(s, |(h, _)| h))
        .collect();
    if hosts.is_empty() {
        return json!({"assigned": false, "kind": "none", "domains": []});
    }
    let dispatcher = hosts.iter().any(|h| h.to_ascii_lowercase().contains("dispatch") && h.to_ascii_lowercase().ends_with(".axis.com"));
    let mut domains: Vec<String> = hosts
        .iter()
        .filter(|h| h.parse::<std::net::IpAddr>().is_err())
        .map(|h| {
            let parts: Vec<&str> = h.split('.').collect();
            let n = parts.len();
            // The last three labels (service.example.com), or the whole name when shorter.
            if n > 3 { parts[n - 3..].join(".") } else { h.to_string() }
        })
        .collect();
    domains.sort();
    domains.dedup();
    json!({
        "assigned": !dispatcher,
        "kind": if dispatcher { "dispatcher" } else { "service" },
        "domains": domains,
        "count": hosts.len(),
    })
}

/// Mode, servers and proxy, from the RemoteService parameters.
pub fn status(client: &VapixClient) -> anyhow::Result<Value> {
    let p = params::parse(&params::list(client, Some(GROUP))?);
    let get = |k: &str| p.get(&format!("{GROUP}.{k}")).map(String::as_str).unwrap_or("");
    let raw = get("Enabled");
    if p.is_empty() || raw.is_empty() {
        bail!("this camera has no O3C client ({GROUP}.Enabled is missing)");
    }
    let mode = Mode::from_param(raw);
    let proxy = get("ProxyServer");
    Ok(json!({
        "mode": mode.map(Mode::name),
        "enabledParam": raw,
        "servers": servers(get("ServerList")),
        "proxy": if proxy.is_empty() { Value::Null } else { json!({"server": proxy, "port": get("ProxyPort")}) },
    }))
}

/// Set the mode and read it back.
pub fn set_mode(client: &VapixClient, mode: Mode) -> anyhow::Result<Value> {
    let before = status(client)?;
    params::update(client, &[(ENABLED, mode.param_value())])?;
    let after = status(client)?;
    if after["mode"] != json!(mode.name()) {
        bail!("the camera reports mode {} after setting {}", after["enabledParam"], mode.name());
    }
    Ok(json!({"before": before["mode"], "after": after["mode"]}))
}

/// The serial number (on Axis devices the MAC address without separators).
pub fn serial(client: &VapixClient) -> anyhow::Result<String> {
    let v = device::get_properties(client, &["SerialNumber"])?;
    let s = v.pointer("/data/propertyList/SerialNumber").and_then(Value::as_str).unwrap_or("");
    if s.is_empty() {
        bail!("the camera did not report a serial number");
    }
    Ok(s.to_string())
}

/// The Owner Authentication Key. Needs administrator rights and direct internet access
/// from the camera: a proxy makes the camera answer error 1100.
pub fn oak(client: &VapixClient) -> anyhow::Result<String> {
    let body = json!({"apiVersion": "1.0", "context": "vapx", "method": "getOAK"});
    let resp = match client.post_json(OAK, &body) {
        Ok(r) => r,
        Err(e) if e.to_string().contains("HTTP 404") => {
            bail!("this camera has no OAK API (oak.cgi); the key is on the paper that came with the camera")
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("VAPIX error 1100") {
                bail!("the camera could not get its OAK (error 1100). It needs direct internet access; a proxy makes this fail");
            }
            return Err(e);
        }
    };
    let key = resp.pointer("/data/oak").and_then(Value::as_str).unwrap_or("");
    if key.is_empty() {
        bail!("the camera answered without an OAK");
    }
    Ok(key.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_map_to_the_enabled_parameter_and_back() {
        for m in [Mode::Always, Mode::OneClick, Mode::Off] {
            assert_eq!(Mode::from_param(m.param_value()), Some(m));
        }
        assert_eq!(Mode::from_param("YES"), Some(Mode::Always));
        assert_eq!(Mode::from_param("maybe"), None);
    }

    #[test]
    fn a_dispatcher_server_list_is_not_assigned() {
        let v = servers("dispatchse1-st.axis.com:443,dispatchse1-st.axis.com:80,192.0.2.120:443");
        assert_eq!(v["assigned"], false);
        assert_eq!(v["kind"], "dispatcher");
    }

    #[test]
    fn a_service_server_list_is_assigned_and_named_by_domain() {
        let v = servers("oneclick-1.cloud.example.com:443,oneclick-2.cloud.example.com:443");
        assert_eq!(v["assigned"], true);
        assert_eq!(v["domains"], json!(["cloud.example.com"]));
        assert_eq!(v["count"], 2);
    }

    #[test]
    fn an_empty_server_list_is_none() {
        assert_eq!(servers("")["kind"], "none");
    }
}
