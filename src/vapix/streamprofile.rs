//! Stream profiles (`streamprofile.cgi`).

use serde_json::{json, Value};

use crate::vapix::client::VapixClient;

/// All stream profiles, or just the one called `name`.
pub fn list_profiles(client: &VapixClient, name: Option<&str>) -> anyhow::Result<Value> {
    let filter: Vec<Value> = name.map(|n| json!({"name": n})).into_iter().collect();
    let resp = client.post_json(
        "/axis-cgi/streamprofile.cgi",
        &json!({"apiVersion": "1.0", "method": "list", "params": {"streamProfileName": filter}}),
    )?;
    Ok(super::data_of(resp))
}

/// Create a profile. `parameters` is URL-encoded, e.g. `resolution=1920x1080&fps=30`.
pub fn create_profile(client: &VapixClient, name: &str, parameters: &str, description: &str) -> anyhow::Result<()> {
    client.post_json(
        "/axis-cgi/streamprofile.cgi",
        &json!({"apiVersion": "1.0", "method": "create", "params": {"streamProfile": [
            {"name": name, "description": description, "parameters": parameters}]}}),
    )?;
    Ok(())
}

pub fn remove_profile(client: &VapixClient, name: &str) -> anyhow::Result<()> {
    client.post_json(
        "/axis-cgi/streamprofile.cgi",
        &json!({"apiVersion": "1.0", "method": "remove", "params": {"streamProfile": [{"name": name}]}}),
    )?;
    Ok(())
}
