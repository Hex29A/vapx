use crate::vapix::client::VapixClient;
use serde_json::json;

/// List disks and their status.
pub fn list_disks(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    client.post_json(
        "/axis-cgi/disks/list.cgi",
        &json!({
            "apiVersion": "1.0",
            "method": "listDisks",
        }),
    )
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
