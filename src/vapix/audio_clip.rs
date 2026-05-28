use anyhow::{bail, Context};
use serde_json::{json, Value};
use std::collections::BTreeMap;

use crate::vapix::client::VapixClient;

const MEDIACLIP_PATH: &str = "/axis-cgi/mediaclip.cgi";

struct MediaClip {
    id: u32,
    name: String,
    location: String,
}

/// List media clips via param.cgi MediaClip group.
pub fn list_clips(client: &VapixClient) -> anyhow::Result<Value> {
    let text = client
        .get_text(
            "/axis-cgi/param.cgi",
            &[("action", "list"), ("group", "MediaClip")],
        )
        .context("Media clip list not available on this camera")?;

    if text.trim().is_empty() || text.trim().starts_with("# Error:") {
        bail!("Media clips not supported on this camera");
    }

    let clips = parse_media_clips(&text);
    let arr: Vec<Value> = clips
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "name": c.name,
                "location": c.location,
            })
        })
        .collect();
    let count = arr.len();
    Ok(json!({ "clips": arr, "count": count }))
}

/// Play a media clip. Accepts clip name (string) or integer ID.
pub fn play_clip(client: &VapixClient, name_or_id: &str) -> anyhow::Result<u32> {
    let id = resolve_id(client, name_or_id)?;
    let id_str = id.to_string();
    client
        .get_text(MEDIACLIP_PATH, &[("action", "play"), ("clip", &id_str)])
        .context("Failed to play clip")?;
    Ok(id)
}

/// Upload an audio clip. The clip_name becomes the clip's display name on the camera.
/// Returns the integer ID assigned by the camera.
pub fn upload_clip(
    client: &VapixClient,
    data: &[u8],
    filename: &str,
    clip_name: &str,
) -> anyhow::Result<u32> {
    // Pass name= query param so the camera stores the correct display name.
    // The multipart field name also doubles as the name, but the query param
    // is authoritative for the MediaClip.M#.Name parameter.
    let resp = client
        .post_multipart_file_with_params(
            MEDIACLIP_PATH,
            &[("action", "upload"), ("name", clip_name)],
            data,
            filename,
            clip_name,
        )
        .context("Failed to upload clip")?;
    parse_clip_id_from_response(&resp)
}

/// Delete a media clip. Accepts clip name (string) or integer ID.
pub fn delete_clip(client: &VapixClient, name_or_id: &str) -> anyhow::Result<u32> {
    let id = resolve_id(client, name_or_id)?;
    let id_str = id.to_string();
    client
        .get_text(MEDIACLIP_PATH, &[("action", "remove"), ("clip", &id_str)])
        .context("Failed to delete clip")?;
    Ok(id)
}

/// Stop any currently playing clip.
pub fn stop_clips(client: &VapixClient) -> anyhow::Result<()> {
    client
        .get_text(MEDIACLIP_PATH, &[("action", "stop")])
        .context("Failed to stop clips")?;
    Ok(())
}

// --- internal helpers ---

fn resolve_id(client: &VapixClient, name_or_id: &str) -> anyhow::Result<u32> {
    if let Ok(id) = name_or_id.parse::<u32>() {
        return Ok(id);
    }
    let text = client
        .get_text(
            "/axis-cgi/param.cgi",
            &[("action", "list"), ("group", "MediaClip")],
        )
        .context("Failed to list clips for name lookup")?;
    let clips = parse_media_clips(&text);
    clips
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(name_or_id))
        .map(|c| c.id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Clip '{}' not found. Use 'vapx clip list' to see available clips.",
                name_or_id
            )
        })
}

fn parse_media_clips(text: &str) -> Vec<MediaClip> {
    let mut map: BTreeMap<u32, (String, String)> = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let parts: Vec<&str> = key.split('.').collect();
        if parts.len() != 4 || parts[0] != "root" || parts[1] != "MediaClip" {
            continue;
        }
        let Some(id_str) = parts[2].strip_prefix('M') else {
            continue;
        };
        let Ok(id) = id_str.parse::<u32>() else {
            continue;
        };
        let entry = map.entry(id).or_insert_with(|| (String::new(), String::new()));
        match parts[3] {
            "Name" => entry.0 = value.to_string(),
            "Location" => entry.1 = value.to_string(),
            _ => {}
        }
    }
    map.into_iter()
        .map(|(id, (name, location))| MediaClip { id, name, location })
        .collect()
}

fn parse_clip_id_from_response(resp: &str) -> anyhow::Result<u32> {
    for line in resp.lines() {
        let lower = line.trim().to_lowercase();
        for prefix in &["uploaded=", "replaced="] {
            if let Some(id_str) = lower.strip_prefix(prefix) {
                return id_str
                    .trim()
                    .parse::<u32>()
                    .context("Failed to parse clip ID from camera response");
            }
        }
    }
    if resp.trim().starts_with("OK") || resp.trim().starts_with("ok") {
        return Ok(0);
    }
    bail!("Unexpected upload response: {}", resp.trim())
}
