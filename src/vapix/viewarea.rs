use crate::vapix::client::VapixClient;
use serde_json::{json, Value};

/// List all view areas.
pub fn list(client: &VapixClient) -> anyhow::Result<Value> {
    client.post_json("/axis-cgi/viewarea/info.cgi", &json!({
        "apiVersion": "1.0",
        "method": "list",
    }))
}

/// Get info for a specific view area by ID.
pub fn get_info(client: &VapixClient, view_area_id: i64) -> anyhow::Result<Value> {
    client.post_json("/axis-cgi/viewarea/info.cgi", &json!({
        "apiVersion": "1.0",
        "method": "getImageSize",
        "params": {
            "viewArea": [{"id": view_area_id}],
        },
    }))
}

/// Set view area geometry.
pub fn set_geometry(
    client: &VapixClient,
    view_area_id: i64,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> anyhow::Result<Value> {
    client.post_json("/axis-cgi/viewarea/info.cgi", &json!({
        "apiVersion": "1.0",
        "method": "setGeometry",
        "params": {
            "viewArea": [{
                "id": view_area_id,
                "rectangularGeometry": {
                    "horizontalOffset": x,
                    "verticalOffset": y,
                    "horizontalSize": width,
                    "verticalSize": height,
                },
            }],
        },
    }))
}
