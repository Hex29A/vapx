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

/// List recordings on storage. Parses the XML response into structured JSON.
pub fn list_recordings(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    let text = client.get_text(
        "/axis-cgi/record/list.cgi",
        &[("recordingid", "all")],
    )?;
    parse_recordings_xml(&text)
}

/// Parse recordings XML into structured JSON.
fn parse_recordings_xml(xml: &str) -> anyhow::Result<serde_json::Value> {
    let doc = roxmltree::Document::parse(xml)?;
    let root = doc.root_element();
    let mut recordings = Vec::new();

    // Get total from <recordings totalnumberofrecordings="N">
    let mut total: i64 = 0;
    if let Some(rec_node) = root.descendants().find(|n| n.has_tag_name("recordings")) {
        if let Some(t) = rec_node.attribute("totalnumberofrecordings") {
            total = t.parse().unwrap_or(0);
        }
    }

    for node in root.descendants().filter(|n| n.has_tag_name("recording")) {
        let mut rec = serde_json::Map::new();
        if let Some(v) = node.attribute("recordingid") {
            rec.insert("id".into(), json!(v));
        }
        if let Some(v) = node.attribute("diskid") {
            rec.insert("disk".into(), json!(v));
        }
        if let Some(v) = node.attribute("starttime") {
            rec.insert("start".into(), json!(v));
        }
        if let Some(v) = node.attribute("starttimelocal") {
            rec.insert("startLocal".into(), json!(v));
        }
        if let Some(v) = node.attribute("stoptime") {
            rec.insert("stop".into(), json!(v));
        }
        if let Some(v) = node.attribute("stoptimelocal") {
            rec.insert("stopLocal".into(), json!(v));
        }
        if let Some(v) = node.attribute("recordingtype") {
            rec.insert("type".into(), json!(v));
        }
        if let Some(v) = node.attribute("recordingstatus") {
            rec.insert("status".into(), json!(v));
        }
        if let Some(v) = node.attribute("source") {
            rec.insert("source".into(), json!(v));
        }
        // Video properties from child element
        if let Some(video) = node.children().find(|n| n.has_tag_name("video")) {
            if let Some(v) = video.attribute("resolution") {
                rec.insert("resolution".into(), json!(v));
            }
            if let Some(v) = video.attribute("framerate") {
                rec.insert("fps".into(), json!(v));
            }
            if let Some(v) = video.attribute("codecname") {
                rec.insert("codec".into(), json!(v));
            }
        }
        recordings.push(serde_json::Value::Object(rec));
    }

    Ok(json!({
        "total": total,
        "recordings": recordings,
    }))
}

/// Get disk health information.
pub fn get_disk_health(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    // Try the modern properties API for each disk
    let disks_resp = list_disks(client)?;
    let mut health_info = Vec::new();

    if let Some(disks) = disks_resp.pointer("/data/disks").and_then(|d| d.as_array()) {
        for disk in disks {
            let disk_id = disk.get("diskID")
                .or_else(|| disk.get("DiskID"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            match get_disk_properties(client, disk_id) {
                Ok(props) => {
                    let mut entry = serde_json::Map::new();
                    entry.insert("id".into(), json!(disk_id));
                    if let Some(data) = props.get("data") {
                        if let Some(d) = data.get("disks").and_then(|d| d.as_array()).and_then(|a| a.first()) {
                            for (k, v) in d.as_object().into_iter().flat_map(|o| o.iter()) {
                                entry.insert(k.clone(), v.clone());
                            }
                        } else {
                            for (k, v) in data.as_object().into_iter().flat_map(|o| o.iter()) {
                                entry.insert(k.clone(), v.clone());
                            }
                        }
                    }
                    health_info.push(serde_json::Value::Object(entry));
                }
                Err(e) => {
                    debug!("Failed to get properties for disk {}: {}", disk_id, e);
                    health_info.push(json!({"id": disk_id, "error": format!("{}", e)}));
                }
            }
        }
    }

    Ok(json!({
        "disks": health_info,
    }))
}

/// Get recording storage info via param.cgi.
pub fn get_storage_params(client: &VapixClient) -> anyhow::Result<String> {
    client.get_text(
        "/axis-cgi/param.cgi",
        &[("action", "list"), ("group", "root.Storage")],
    )
}
