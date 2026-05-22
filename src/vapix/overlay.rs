use serde_json::{json, Value};

use crate::vapix::client::VapixClient;

const OVERLAY_CGI: &str = "/axis-cgi/dynamicoverlay/dynamicoverlay.cgi";

/// List all overlays.
pub fn list(client: &VapixClient) -> anyhow::Result<Value> {
    let body = json!({
        "apiVersion": "1.0",
        "method": "list",
        "params": {}
    });
    client.post_json(OVERLAY_CGI, &body)
}

/// Add a text overlay.
pub fn add_text(
    client: &VapixClient,
    camera: u32,
    text: &str,
    position: Option<&str>,
    font_size: Option<u32>,
    text_color: Option<&str>,
    bg_color: Option<&str>,
) -> anyhow::Result<Value> {
    let mut params = json!({
        "camera": camera,
        "text": text,
    });
    if let Some(pos) = position {
        params["position"] = json!(pos);
    }
    if let Some(fs) = font_size {
        params["fontSize"] = json!(fs);
    }
    if let Some(tc) = text_color {
        params["textColor"] = json!(tc);
    }
    if let Some(bg) = bg_color {
        params["textBGColor"] = json!(bg);
    }

    let body = json!({
        "apiVersion": "1.0",
        "method": "addText",
        "params": params,
    });
    client.post_json(OVERLAY_CGI, &body)
}

/// Update a text overlay.
pub fn set_text(
    client: &VapixClient,
    identity: u32,
    text: Option<&str>,
    position: Option<&str>,
    font_size: Option<u32>,
    text_color: Option<&str>,
    bg_color: Option<&str>,
) -> anyhow::Result<Value> {
    let mut params = json!({
        "identity": identity,
    });
    if let Some(t) = text {
        params["text"] = json!(t);
    }
    if let Some(pos) = position {
        params["position"] = json!(pos);
    }
    if let Some(fs) = font_size {
        params["fontSize"] = json!(fs);
    }
    if let Some(tc) = text_color {
        params["textColor"] = json!(tc);
    }
    if let Some(bg) = bg_color {
        params["textBGColor"] = json!(bg);
    }

    let body = json!({
        "apiVersion": "1.0",
        "method": "setText",
        "params": params,
    });
    client.post_json(OVERLAY_CGI, &body)
}

/// Remove an overlay by identity.
pub fn remove(client: &VapixClient, identity: u32) -> anyhow::Result<Value> {
    let body = json!({
        "apiVersion": "1.0",
        "method": "remove",
        "params": {
            "identity": identity,
        }
    });
    client.post_json(OVERLAY_CGI, &body)
}

/// Get overlay capabilities.
pub fn get_capabilities(client: &VapixClient) -> anyhow::Result<Value> {
    let body = json!({
        "apiVersion": "1.0",
        "method": "getOverlayCapabilities",
    });
    client.post_json(OVERLAY_CGI, &body)
}
