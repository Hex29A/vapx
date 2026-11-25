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
        return Err(ptz_error(&text));
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

/// The error for a `ptz.cgi` "Error:" reply. A fixed camera answers that the
/// camera "does not map to any serial port", which means it has no PTZ.
fn ptz_error(text: &str) -> anyhow::Error {
    let text = text.trim();
    if text.contains("does not map to any serial port") {
        anyhow::anyhow!("This camera has no PTZ (the camera answered: {})", text.replace(['\r', '\n'], " ").trim())
    } else {
        anyhow::anyhow!("PTZ: {}", text)
    }
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
        return Err(ptz_error(&text));
    }
    Ok(())
}

/// Current position as `{pan, tilt, zoom}` (numbers).
pub fn position(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    Ok(parse_position(&query(client, "position", None)?))
}

fn parse_position(text: &str) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for line in text.lines() {
        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim().to_lowercase();
            if matches!(key.as_str(), "pan" | "tilt" | "zoom") {
                if let Some(n) = val.trim().parse::<f64>().ok().and_then(serde_json::Number::from_f64) {
                    out.insert(key, serde_json::Value::Number(n));
                }
            }
        }
    }
    serde_json::Value::Object(out)
}

#[cfg(test)]
mod error_tests {
    use super::ptz_error;

    #[test]
    fn a_fixed_camera_is_said_to_have_no_ptz() {
        let msg = ptz_error("Error:\r\nCamera 1 does not map to any serial port").to_string();
        assert!(msg.starts_with("This camera has no PTZ"), "{msg}");
        assert!(msg.contains("Camera 1 does not map"), "{msg}");
    }

    #[test]
    fn other_errors_keep_the_camera_text() {
        assert_eq!(ptz_error("Error:\nInvalid pan").to_string(), "PTZ: Error:\nInvalid pan");
    }
}
