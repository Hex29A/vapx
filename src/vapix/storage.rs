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
/// Tries `disks/properties.cgi` first; on 404 falls back to extracting
/// available data from `disks/list.cgi` (which returns totalsize, freesize,
/// status, etc. on cameras where `properties.cgi` is absent).
pub fn get_disk_properties(client: &VapixClient, disk_id: &str) -> anyhow::Result<serde_json::Value> {
    match client.post_json(
        "/axis-cgi/disks/properties.cgi",
        &json!({
            "apiVersion": "1.0",
            "method": "getDiskProperties",
            "params": {
                "diskID": disk_id,
            },
        }),
    ) {
        Ok(resp) => Ok(resp),
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("404") {
                debug!(
                    "disks/properties.cgi returned 404 for {}, falling back to list.cgi",
                    disk_id
                );
                get_disk_properties_from_list(client, disk_id)
            } else {
                Err(e)
            }
        }
    }
}

/// List disks via the classic GET `disks/list.cgi` XML endpoint — NOT the
/// same data as `list_disks()` above. `list_disks()` uses the modern JSON
/// `listDisks` method, whose response omits totalsize/freesize on some
/// AXIS OS 12.x firmware (issue #37); the classic GET XML endpoint returns
/// them (`<disk diskid="..." totalsize="..." freesize="..." status="..."
/// .../>`) even on the same cameras. vapx-mcp's `get_disk_status` already
/// relies on this GET form and returns full size data — this mirrors that.
/// Used as the data source for both storage-fallback paths below so neither
/// silently drops size info again.
fn list_disks_xml(client: &VapixClient) -> anyhow::Result<Vec<serde_json::Map<String, serde_json::Value>>> {
    let xml = client.get_text("/axis-cgi/disks/list.cgi", &[("diskid", "all")])?;
    let doc = roxmltree::Document::parse(&xml)?;

    Ok(doc
        .descendants()
        .filter(|n| n.has_tag_name("disk"))
        .map(|node| {
            let mut disk = serde_json::Map::new();
            for attr in node.attributes() {
                disk.insert(attr.name().to_string(), json!(attr.value()));
            }
            disk
        })
        .collect())
}

/// Fallback: extract properties for a single disk from the `disks/list.cgi`
/// XML response (see `list_disks_xml`).
fn get_disk_properties_from_list(
    client: &VapixClient,
    disk_id: &str,
) -> anyhow::Result<serde_json::Value> {
    let disks = list_disks_xml(client)?;
    let disk = disks
        .into_iter()
        .find(|d| d.get("diskid").and_then(|v| v.as_str()) == Some(disk_id))
        .ok_or_else(|| anyhow::anyhow!("Disk '{}' not found", disk_id))?;

    Ok(json!({
        "data": {
            "disks": [serde_json::Value::Object(disk)],
            "source": "list_cgi_fallback",
        }
    }))
}

/// List recordings on storage. Parses the XML response into structured JSON.
/// `max` controls the maximum number of recordings to fetch (default 1000).
pub fn list_recordings(client: &VapixClient, max: u32) -> anyhow::Result<serde_json::Value> {
    let max_str = max.to_string();
    let text = client.get_text(
        "/axis-cgi/record/list.cgi",
        &[("recordingid", "all"), ("maxnumberofrecordings", &max_str)],
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
/// Tries `properties.cgi` per disk; on 404 falls back to `list.cgi` data
/// which includes totalsize, freesize, status, and other health-relevant fields.
pub fn get_disk_health(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    // Enumerate via the XML disks/list.cgi form (see list_disks_xml) rather
    // than the JSON listDisks method — its disk objects are what feed the
    // properties.cgi-404 fallback below, and only the XML form carries
    // totalsize/freesize on firmware where properties.cgi is absent (#37).
    let disks = list_disks_xml(client)?;
    let mut health_info = Vec::new();
    let mut properties_available = true;

    for disk in &disks {
        let disk_id = disk.get("diskid")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        if !properties_available {
            // Already know properties.cgi is 404, use list data directly
            health_info.push(build_health_from_list(disk_id, disk));
            continue;
        }

        match client.post_json(
            "/axis-cgi/disks/properties.cgi",
            &json!({
                "apiVersion": "1.0",
                "method": "getDiskProperties",
                "params": { "diskID": disk_id },
            }),
        ) {
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
                let msg = format!("{}", e);
                if msg.contains("404") {
                    debug!(
                        "disks/properties.cgi returned 404, using list.cgi data for health"
                    );
                    properties_available = false;
                    health_info.push(build_health_from_list(disk_id, disk));
                } else {
                    debug!("Failed to get properties for disk {}: {}", disk_id, e);
                    health_info.push(json!({"id": disk_id, "error": format!("{}", e)}));
                }
            }
        }
    }

    let mut result = json!({ "disks": health_info });
    if !properties_available {
        result.as_object_mut().unwrap().insert(
            "note".into(),
            json!("Health data from list.cgi (properties.cgi not available on this firmware)"),
        );
    }

    Ok(result)
}

/// Build a health entry from list.cgi disk data (fallback when properties.cgi is absent).
fn build_health_from_list(disk_id: &str, disk: &serde_json::Map<String, serde_json::Value>) -> serde_json::Value {
    let mut entry = serde_json::Map::new();
    entry.insert("id".into(), json!(disk_id));
    // Copy all available fields from list.cgi (includes totalsize/freesize — #37)
    for (k, v) in disk {
        if k != "diskid" {
            entry.insert(k.clone(), v.clone());
        }
    }
    entry.insert("source".into(), json!("list_cgi_fallback"));
    serde_json::Value::Object(entry)
}

/// Get recording storage info via param.cgi.
pub fn get_storage_params(client: &VapixClient) -> anyhow::Result<String> {
    client.get_text(
        "/axis-cgi/param.cgi",
        &[("action", "list"), ("group", "root.Storage")],
    )
}
