//! Camera lookup for tool calls: the same `cameras.yaml` (with env-var
//! substitution) the CLI uses, read again on every call so edits made with
//! `vapx config` show up without restarting the server.

use anyhow::{anyhow, bail};
use serde_json::{json, Value};

use crate::config::cameras::load_cameras;
use crate::config::credentials::Credentials;
use crate::vapix::client::VapixClient;

pub struct Cam {
    pub host: String,
    pub creds: Credentials,
    pub client: VapixClient,
}

/// Camera IDs are the keys of `cameras.yaml`, as in the Python server.
pub fn open(id: &str) -> anyhow::Result<Cam> {
    let config = load_cameras()?.ok_or_else(|| anyhow!("No cameras.yaml found"))?;
    let entry = match config.cameras.get(id) {
        Some(e) => e,
        None => {
            let mut ids: Vec<&str> = config.cameras.keys().map(String::as_str).collect();
            ids.sort_unstable();
            bail!("Camera '{}' not found. Available cameras: {}", id, ids.join(", "));
        }
    };
    if !entry.enabled {
        bail!("Camera '{}' is disabled in cameras.yaml", id);
    }
    let user = config
        .effective_user(entry)
        .ok_or_else(|| anyhow!("Camera '{}' has no user in cameras.yaml", id))?;
    let pass = entry
        .pass
        .clone()
        .or_else(|| crate::cmd::config::keyring_lookup(id))
        .ok_or_else(|| anyhow!("Camera '{}' has no password in cameras.yaml or the keyring", id))?;
    let https = config.effective_https(entry);
    let creds = Credentials {
        user,
        pass,
        https,
        verify_ssl: config.effective_verify_ssl(entry),
        port: entry.port.unwrap_or(if https { 443 } else { 80 }),
        timeout: config.effective_timeout(entry),
    };
    let client = VapixClient::new(&entry.host, creds.port, creds.clone(), creds.timeout);
    Ok(Cam { host: entry.host.clone(), creds, client })
}

/// All enabled cameras, sorted by ID: `[{id, name, host, capabilities}]`.
///
/// `capabilities` is empty: the CLI's config has no capability list, and the
/// server does not probe cameras just to list them. Calling a tool on a camera
/// that lacks the feature gives the camera's own error.
pub fn list() -> anyhow::Result<Value> {
    let config = load_cameras()?.ok_or_else(|| anyhow!("No cameras.yaml found"))?;
    let mut rows: Vec<(&String, &crate::config::cameras::CameraEntry)> =
        config.cameras.iter().filter(|(_, e)| e.enabled).collect();
    rows.sort_by(|a, b| a.0.cmp(b.0));
    Ok(Value::Array(
        rows.into_iter()
            .map(|(id, e)| json!({"id": id, "name": id, "host": e.host, "capabilities": []}))
            .collect(),
    ))
}
