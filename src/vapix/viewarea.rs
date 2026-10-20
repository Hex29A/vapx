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

/// View areas in the shape the MCP tools use: `[{id, camera, source,
/// configurable, canvas_size?, geometry?, min_size?, max_size?, grid?}]`.
pub fn list_normalized(client: &VapixClient) -> anyhow::Result<Value> {
    let resp = list(client)?;
    Ok(normalize(&resp))
}

fn normalize(resp: &Value) -> Value {
    let areas = resp.pointer("/data/viewAreas").and_then(Value::as_array).cloned().unwrap_or_default();
    Value::Array(
        areas
            .iter()
            .map(|a| {
                let mut e = serde_json::Map::new();
                e.insert("id".into(), a.get("id").cloned().unwrap_or(Value::Null));
                e.insert("camera".into(), a.get("camera").cloned().unwrap_or(Value::Null));
                e.insert("source".into(), a.get("source").cloned().unwrap_or(Value::Null));
                e.insert("configurable".into(), a.get("configurable").cloned().unwrap_or(Value::Bool(false)));
                if let Some(v) = a.get("canvasSize") {
                    e.insert("canvas_size".into(), v.clone());
                }
                if let Some(g) = a.get("rectangularGeometry") {
                    e.insert(
                        "geometry".into(),
                        json!({
                            "horizontal_offset": g.get("horizontalOffset"),
                            "vertical_offset": g.get("verticalOffset"),
                            "horizontal_size": g.get("horizontalSize"),
                            "vertical_size": g.get("verticalSize"),
                        }),
                    );
                }
                for (from, to) in [("minSize", "min_size"), ("maxSize", "max_size"), ("grid", "grid")] {
                    if let Some(v) = a.get(from) {
                        e.insert(to.into(), v.clone());
                    }
                }
                Value::Object(e)
            })
            .collect(),
    )
}

/// Set a view area's rectangular geometry through `configure.cgi`.
pub fn configure_geometry(
    client: &VapixClient,
    view_area_id: i64,
    horizontal_offset: i64,
    vertical_offset: i64,
    horizontal_size: i64,
    vertical_size: i64,
) -> anyhow::Result<Value> {
    let resp = client.post_json("/axis-cgi/viewarea/configure.cgi", &json!({
        "apiVersion": "1.0",
        "method": "setGeometry",
        "params": {
            "viewAreaId": view_area_id,
            "rectangularGeometry": {
                "horizontalOffset": horizontal_offset,
                "verticalOffset": vertical_offset,
                "horizontalSize": horizontal_size,
                "verticalSize": vertical_size,
            },
        },
    }))?;
    Ok(super::data_of(resp))
}

/// Reset a view area to its factory geometry (the full canvas).
pub fn reset_geometry(client: &VapixClient, view_area_id: i64) -> anyhow::Result<Value> {
    let resp = client.post_json("/axis-cgi/viewarea/configure.cgi", &json!({
        "apiVersion": "1.0",
        "method": "resetGeometry",
        "params": {"viewAreaId": view_area_id},
    }))?;
    Ok(super::data_of(resp))
}
