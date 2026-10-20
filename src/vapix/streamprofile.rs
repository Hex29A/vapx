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
