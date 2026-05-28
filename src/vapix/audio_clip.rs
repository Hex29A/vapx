use anyhow::Context;
use serde_json::{json, Value};

use crate::vapix::client::VapixClient;

const LIST_PATH: &str = "/axis-cgi/audio/list.cgi";
const PLAY_PATH: &str = "/axis-cgi/audio/play.cgi";
const UPLOAD_PATH: &str = "/axis-cgi/audio/upload.cgi";
const REMOVE_PATH: &str = "/axis-cgi/audio/remove.cgi";

/// List audio clips stored on the camera.
/// Returns a JSON array of objects with "name" and "location" fields.
pub fn list_clips(client: &VapixClient) -> anyhow::Result<Value> {
    let text = client
        .get_text(LIST_PATH, &[])
        .context("Audio clip list not available on this camera")?;

    let clips: Vec<Value> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| {
            let name = l.strip_prefix("clip:").unwrap_or(l);
            json!({
                "name": name,
                "location": format!("clip:{}", name),
            })
        })
        .collect();

    Ok(json!({ "clips": clips, "count": clips.len() }))
}

/// Play an audio clip on the camera's built-in speaker.
/// Accepts the clip name with or without the "clip:" prefix.
pub fn play_clip(client: &VapixClient, name: &str) -> anyhow::Result<()> {
    let location = if name.starts_with("clip:") {
        name.to_string()
    } else {
        format!("clip:{}", name)
    };
    client
        .get_text(PLAY_PATH, &[("location", &location)])
        .context("Failed to play audio clip")?;
    Ok(())
}

/// Upload an audio clip file to the camera.
/// The clip name is derived from the filename (without extension).
pub fn upload_clip(client: &VapixClient, data: &[u8], filename: &str) -> anyhow::Result<()> {
    client
        .post_multipart_file(UPLOAD_PATH, data, filename)
        .context("Failed to upload audio clip")?;
    Ok(())
}

/// Delete an audio clip from the camera.
/// Accepts the clip name with or without the "clip:" prefix.
pub fn delete_clip(client: &VapixClient, name: &str) -> anyhow::Result<()> {
    let clipname = name.strip_prefix("clip:").unwrap_or(name);
    client
        .get_text(REMOVE_PATH, &[("clipname", clipname)])
        .context("Failed to delete audio clip")?;
    Ok(())
}
