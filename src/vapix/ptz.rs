use anyhow::{bail, Context};

use crate::vapix::client::VapixClient;

/// Send a PTZ movement command (home, up, down, left, right, stop, etc.).
pub fn move_direction(
    client: &VapixClient,
    direction: &str,
    camera: Option<u8>,
) -> anyhow::Result<()> {
    let cam_str;
    let mut params: Vec<(&str, &str)> = vec![("move", direction)];
    if let Some(c) = camera {
        cam_str = c.to_string();
        params.push(("camera", &cam_str));
    }
    ptz_command(client, &params)
}

/// Parameters for absolute/relative PTZ positioning.
pub struct GotoParams {
    pub pan: Option<f64>,
    pub tilt: Option<f64>,
    pub zoom: Option<i32>,
    pub rpan: Option<f64>,
    pub rtilt: Option<f64>,
    pub rzoom: Option<i32>,
    pub speed: Option<i32>,
    pub camera: Option<u8>,
}

/// Send an absolute/relative PTZ positioning command.
pub fn goto(client: &VapixClient, p: GotoParams) -> anyhow::Result<()> {
    let mut owned: Vec<(&str, String)> = Vec::new();
    if let Some(v) = p.pan {
        owned.push(("pan", v.to_string()));
    }
    if let Some(v) = p.tilt {
        owned.push(("tilt", v.to_string()));
    }
    if let Some(v) = p.zoom {
        owned.push(("zoom", v.to_string()));
    }
    if let Some(v) = p.rpan {
        owned.push(("rpan", v.to_string()));
    }
    if let Some(v) = p.rtilt {
        owned.push(("rtilt", v.to_string()));
    }
    if let Some(v) = p.rzoom {
        owned.push(("rzoom", v.to_string()));
    }
    if let Some(v) = p.speed {
        owned.push(("speed", v.to_string()));
    }
    if let Some(c) = p.camera {
        owned.push(("camera", c.to_string()));
    }
    if owned.is_empty() {
        bail!("No positioning parameters specified");
    }

    let params: Vec<(&str, &str)> = owned.iter().map(|(k, v)| (*k, v.as_str())).collect();
    ptz_command(client, &params)
}

/// Go to a named server preset position.
pub fn goto_preset(
    client: &VapixClient,
    name: &str,
    camera: Option<u8>,
) -> anyhow::Result<()> {
    let cam_str;
    let mut params: Vec<(&str, &str)> = vec![("gotoserverpresetname", name)];
    if let Some(c) = camera {
        cam_str = c.to_string();
        params.push(("camera", &cam_str));
    }
    ptz_command(client, &params)
}

/// Save the current PTZ position as a named server preset.
pub fn save_preset(
    client: &VapixClient,
    name: &str,
    camera: Option<u8>,
) -> anyhow::Result<()> {
    let cam_str;
    let mut params: Vec<(&str, &str)> = vec![("setserverpresetname", name)];
    if let Some(c) = camera {
        cam_str = c.to_string();
        params.push(("camera", &cam_str));
    }
    ptz_command(client, &params)
}

/// Query PTZ status (position, limits, presetposcam, speed).
pub fn query(
    client: &VapixClient,
    what: &str,
    camera: Option<u8>,
) -> anyhow::Result<String> {
    let cam_str;
    let mut params: Vec<(&str, &str)> = vec![("query", what)];
    if let Some(c) = camera {
        cam_str = c.to_string();
        params.push(("camera", &cam_str));
    }
    let text = client.get_text("/axis-cgi/com/ptz.cgi", &params)?;
    if text.starts_with("Error:") {
        bail!("PTZ: {}", text.trim());
    }
    Ok(text)
}

/// Get info about available PTZ commands (info=1).
pub fn info(client: &VapixClient, camera: Option<u8>) -> anyhow::Result<String> {
    let cam_str;
    let mut params: Vec<(&str, &str)> = vec![("info", "1")];
    if let Some(c) = camera {
        cam_str = c.to_string();
        params.push(("camera", &cam_str));
    }
    client.get_text("/axis-cgi/com/ptz.cgi", &params)
}

/// Execute a PTZ command and validate the response.
/// Success: HTTP 204 No Content.
/// Error: HTTP 200 with "Error: ..." body.
fn ptz_command(client: &VapixClient, params: &[(&str, &str)]) -> anyhow::Result<()> {
    let resp = client.get("/axis-cgi/com/ptz.cgi", params)?;
    let status = resp.status();
    if status.as_u16() == 204 {
        return Ok(());
    }
    let text = resp.text().context("Failed to read PTZ response")?;
    if text.contains("Error:") {
        bail!("PTZ: {}", text.trim());
    }
    Ok(())
}
