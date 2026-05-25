use anyhow::bail;

use crate::vapix::client::VapixClient;

/// Fetch day/night mode parameters (ImageSource.I0.DayNight group).
pub fn get_daynight(client: &VapixClient) -> anyhow::Result<String> {
    let text = client.get_text(
        "/axis-cgi/param.cgi",
        &[("action", "list"), ("group", "root.ImageSource.I0.DayNight")],
    )?;
    if text.starts_with("# Error:") {
        bail!("param.cgi: {}", text.trim());
    }
    Ok(text)
}

/// Fetch imaging/sensor parameters (ImageSource.I0.Sensor group).
pub fn get_imaging(client: &VapixClient) -> anyhow::Result<String> {
    let text = client.get_text(
        "/axis-cgi/param.cgi",
        &[("action", "list"), ("group", "root.ImageSource.I0.Sensor")],
    )?;
    if text.starts_with("# Error:") {
        bail!("param.cgi: {}", text.trim());
    }
    Ok(text)
}

/// Fetch light/IR illuminator parameters.
/// Tries Properties.LightControl first, then falls back to root.LightControl.
pub fn get_light(client: &VapixClient) -> anyhow::Result<String> {
    let text = client.get_text(
        "/axis-cgi/param.cgi",
        &[("action", "list"), ("group", "root.Properties.LightControl")],
    )?;
    if text.starts_with("# Error:") {
        // Try root.LightControl as fallback
        let text2 = client.get_text(
            "/axis-cgi/param.cgi",
            &[("action", "list"), ("group", "root.LightControl")],
        )?;
        if text2.starts_with("# Error:") {
            bail!("Light control parameters not available on this camera");
        }
        return Ok(text2);
    }
    // Also fetch root.LightControl for intensity/state values
    let text2 = client.get_text(
        "/axis-cgi/param.cgi",
        &[("action", "list"), ("group", "root.LightControl")],
    );
    match text2 {
        Ok(t2) if !t2.starts_with("# Error:") => Ok(format!("{}{}", text, t2)),
        _ => Ok(text),
    }
}

/// Fetch video motion detection parameters.
pub fn get_vmd(client: &VapixClient) -> anyhow::Result<String> {
    // Try modern VMD4 parameters first
    let text = client.get_text(
        "/axis-cgi/param.cgi",
        &[("action", "list"), ("group", "root.Properties.Motion")],
    )?;
    // Also fetch motion detection profile params if available
    let profiles = client.get_text(
        "/axis-cgi/param.cgi",
        &[("action", "list"), ("group", "root.Motion")],
    );
    let mut result = String::new();
    if !text.starts_with("# Error:") {
        result.push_str(&text);
    }
    if let Ok(p) = profiles {
        if !p.starts_with("# Error:") {
            result.push_str(&p);
        }
    }
    if result.is_empty() {
        bail!("Video motion detection parameters not available on this camera");
    }
    Ok(result)
}

/// Fetch audio parameters (AudioSource group).
pub fn get_audio(client: &VapixClient) -> anyhow::Result<String> {
    let text = client.get_text(
        "/axis-cgi/param.cgi",
        &[("action", "list"), ("group", "root.AudioSource")],
    )?;
    if text.starts_with("# Error:") {
        // Try Properties.Audio as fallback
        let text2 = client.get_text(
            "/axis-cgi/param.cgi",
            &[("action", "list"), ("group", "root.Properties.Audio")],
        )?;
        if text2.starts_with("# Error:") {
            bail!("Audio parameters not available on this camera");
        }
        return Ok(text2);
    }
    // Also fetch Properties.Audio for capabilities
    let text2 = client.get_text(
        "/axis-cgi/param.cgi",
        &[("action", "list"), ("group", "root.Properties.Audio")],
    );
    match text2 {
        Ok(t2) if !t2.starts_with("# Error:") => Ok(format!("{}{}", text, t2)),
        _ => Ok(text),
    }
}
