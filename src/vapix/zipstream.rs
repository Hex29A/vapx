use anyhow::bail;

use crate::vapix::client::VapixClient;

/// List available ZipStream profiles via the XML CGI.
/// Returns the raw XML response.
pub fn list_profiles(client: &VapixClient) -> anyhow::Result<String> {
    let text = client.get_text("/axis-cgi/zipstream/listprofiles.cgi", &[])?;
    let lower = text.to_lowercase();
    if lower.contains("<error>") || lower.contains("not found") {
        bail!("ZipStream API not available on this camera. Use 'vapx discover' to check supported APIs.");
    }
    Ok(text)
}

/// Set ZipStream profile and level.
/// profile: "classic", "storage", or "networkloadbalancing"
/// level: 0-100 (strength)
pub fn set_profile(client: &VapixClient, profile: &str, level: u32) -> anyhow::Result<String> {
    let text = client.get_text(
        "/axis-cgi/zipstream/setprofile.cgi",
        &[("profile", profile), ("level", &level.to_string())],
    )?;
    let lower = text.to_lowercase();
    if lower.contains("<error>") || lower.contains("not found") {
        bail!("ZipStream API not available on this camera. Use 'vapx discover' to check supported APIs.");
    }
    Ok(text)
}

/// ZipStream profiles and the maximum level as JSON, from the camera's XML.
pub fn parse_profiles(xml: &str) -> anyhow::Result<serde_json::Value> {
    let doc = roxmltree::Document::parse(xml)?;
    let root = doc.root_element();
    let mut profiles = Vec::new();

    for profile_node in root.descendants().filter(|n| n.has_tag_name("Profile")) {
        let mut profile = serde_json::Map::new();
        if let Some(name) = profile_node.attribute("name") {
            profile.insert("name".into(), serde_json::json!(name));
        }
        // Check for level attribute or child element
        if let Some(level) = profile_node.attribute("level") {
            profile.insert("level".into(), serde_json::json!(level));
        }
        for child in profile_node.children().filter(|n| n.is_element()) {
            let tag = child.tag_name().name();
            if let Some(text) = child.text() {
                profile.insert(tag.to_string(), serde_json::json!(text.trim()));
            }
        }
        profiles.push(serde_json::Value::Object(profile));
    }

    // Also capture MaxLevel if present
    let mut result = serde_json::Map::new();
    if let Some(max_level_node) = root.descendants().find(|n| n.has_tag_name("MaxLevel")) {
        if let Some(text) = max_level_node.text() {
            result.insert(
                "maxLevel".into(),
                serde_json::json!(text.trim().parse::<i64>().unwrap_or(0)),
            );
        }
    }
    result.insert("profiles".into(), serde_json::json!(profiles));
    Ok(serde_json::Value::Object(result))
}
