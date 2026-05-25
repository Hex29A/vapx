use crate::vapix::client::VapixClient;
use serde_json::json;
use tracing::debug;

/// List disks and their status.
/// Tries modern JSON API first (/axis-cgi/disks/list.cgi), falls back to
/// param.cgi for cameras running firmware older than AXIS OS 10.x.
pub fn list_disks(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    match client.post_json(
        "/axis-cgi/disks/list.cgi",
        &json!({
            "apiVersion": "1.0",
            "method": "listDisks",
        }),
    ) {
        Ok(resp) => Ok(resp),
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("400") || msg.contains("404") {
                debug!("Modern disk API failed ({}), falling back to param.cgi", msg);
                list_disks_legacy(client)
            } else {
                Err(e)
            }
        }
    }
}

/// Legacy disk listing via param.cgi for pre-AXIS OS 10.x cameras.
fn list_disks_legacy(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    let text = get_storage_params(client)?;
    let mut disks: std::collections::HashMap<String, serde_json::Map<String, serde_json::Value>> =
        std::collections::HashMap::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            // Parse root.Storage.S0.DiskID=SD_DISK etc.
            let parts: Vec<&str> = key.split('.').collect();
            if parts.len() >= 4 && parts[0] == "root" && parts[1] == "Storage" {
                let slot = parts[2]; // S0, S1, etc.
                let prop = parts[3..].join(".");
                disks
                    .entry(slot.to_string())
                    .or_default()
                    .insert(prop, json!(val));
            }
        }
    }

    let disk_list: Vec<serde_json::Value> = disks
        .into_iter()
        .map(|(slot, props)| {
            let mut obj = props;
            obj.insert("slot".to_string(), json!(slot));
            serde_json::Value::Object(obj)
        })
        .collect();

    Ok(json!({
        "data": {
            "disks": disk_list,
            "source": "legacy_param_cgi",
        }
    }))
}

/// Get disk properties (health, usage, etc.).
pub fn get_disk_properties(client: &VapixClient, disk_id: &str) -> anyhow::Result<serde_json::Value> {
    client.post_json(
        "/axis-cgi/disks/properties.cgi",
        &json!({
            "apiVersion": "1.0",
            "method": "getDiskProperties",
            "params": {
                "diskID": disk_id,
            },
        }),
    )
}

/// List recordings on storage.
pub fn list_recordings(client: &VapixClient) -> anyhow::Result<String> {
    client.get_text(
        "/axis-cgi/record/list.cgi",
        &[("recordingid", "all")],
    )
}

/// Get recording storage info via param.cgi.
pub fn get_storage_params(client: &VapixClient) -> anyhow::Result<String> {
    client.get_text(
        "/axis-cgi/param.cgi",
        &[("action", "list"), ("group", "root.Storage")],
    )
}
