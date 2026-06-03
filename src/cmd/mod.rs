pub mod acap;
pub mod audio;
pub mod clip;
pub mod audit;
pub mod backup;
pub mod batch;
pub mod cert;
pub mod config;
pub mod daynight;
pub mod diff;
pub mod discover;
pub mod events;
pub mod fw;
pub mod health;
pub mod hw;
pub mod imaging;
pub mod info;
pub mod light;
pub mod log;
pub mod mqtt;
pub mod net;
pub mod overlay;
pub mod param;
pub mod pass;
pub mod ptz;
pub mod rule;
pub mod selftest;
pub mod signedvideo;
pub mod snap;
pub mod storage;
pub mod stream;
pub mod streamstatus;
pub mod temp;
pub mod template;
pub mod time;
pub mod user;
pub mod viewarea;
pub mod vmd;
pub mod watch;
pub mod zipstream;

use crate::config::credentials::{self, Credentials};
use crate::vapix::client::VapixClient;

/// Resolve camera credentials and host from CLI args or cameras.yaml.
pub(crate) fn resolve_cam(
    host: &str,
    user: Option<&str>,
    pass: Option<&str>,
    port: Option<u16>,
    insecure: bool,
) -> anyhow::Result<(Credentials, String)> {
    credentials::resolve(host, user, pass, port, insecure)
}

/// Create a VapixClient from resolved credentials.
pub(crate) fn make_client(
    host: &str,
    creds: Credentials,
    timeout: Option<u64>,
) -> VapixClient {
    let t = timeout.unwrap_or(creds.timeout);
    VapixClient::new(host, creds.port, creds, t)
}

/// Resolve a target string (group name or comma-separated camera names) to a list of camera names.
/// Cameras with `enabled: false` are filtered out and logged.
pub(crate) fn resolve_targets(
    config: &crate::config::cameras::CamerasConfig,
    input: &str,
) -> anyhow::Result<Vec<String>> {
    // Check if it's a group name
    let raw = if let Some(members) = config.groups.get(input) {
        members.clone()
    } else {
        // Treat as comma-separated camera names/hosts
        let names: Vec<String> = input.split(',').map(|s| s.trim().to_string()).collect();

        // Validate all names exist in config
        for name in &names {
            if config.find(name).is_none() {
                anyhow::bail!("Camera '{}' not found in cameras.yaml", name);
            }
        }
        names
    };

    // Filter out disabled cameras
    let mut enabled = Vec::new();
    for name in &raw {
        if let Some((_, entry)) = config.find(name) {
            if entry.enabled {
                enabled.push(name.clone());
            } else {
                tracing::info!("Skipping '{}': disabled in config", name);
            }
        } else {
            enabled.push(name.clone());
        }
    }

    Ok(enabled)
}

/// Parse param.cgi key=value text into a JSON map.
pub fn param_to_json(text: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.to_string(), serde_json::Value::String(v.to_string()));
        }
    }
    map
}
