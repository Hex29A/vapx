//! Tools that change a camera, or reach out to it in ways the read-only tools
//! do not (events, snapshots of all cameras, exports). Same names, parameters
//! and result texts as the Python server.

use std::path::PathBuf;

use anyhow::{anyhow, bail, Context};
use base64::Engine;
use serde_json::{json, Map, Value};

use super::{json_out, text_out, Args, Handler, Out};
use crate::mcp::camera;
use crate::vapix::{
    analyticsmetadata, applications, audio, audio_clip, capturemode, clearview, daynight, events, geolocation,
    guardtour, io, lightcontrol, mqtt, overlay, privacymask, ptz, siren, storage, streamprofile, system, time,
    viewarea, vmd,
};

pub fn handlers() -> Vec<(&'static str, Handler)> {
    vec![
        ("ptz_move", ptz_move),
        ("ptz_relative", ptz_relative),
        ("ptz_home", ptz_home),
        ("ptz_preset", ptz_preset),
        ("set_io_port", set_io_port),
        ("toggle_light", toggle_light),
        ("add_overlay", add_overlay),
        ("remove_overlay", remove_overlay),
        ("set_motion_config", set_motion_config),
        ("start_guard_tour", start_guard_tour),
        ("stop_guard_tour", stop_guard_tour),
        ("activate_siren", activate_siren),
        ("stop_siren", stop_siren),
        ("export_recording", export_recording),
        ("start_clear_view", start_clear_view),
        ("stop_clear_view", stop_clear_view),
        ("add_privacy_mask", add_privacy_mask),
        ("remove_privacy_mask", remove_privacy_mask),
        ("set_timezone", set_timezone),
        ("set_daynight_config", set_daynight_config),
        ("create_stream_profile", create_stream_profile),
        ("remove_stream_profile", remove_stream_profile),
        ("set_geolocation", set_geolocation),
        ("set_audio_settings", set_audio_settings),
        ("poll_events", poll_events),
        ("set_capture_mode", set_capture_mode),
        ("set_ntp_config", set_ntp_config),
        ("set_analytics_producers", set_analytics_producers),
        ("configure_mqtt", configure_mqtt),
        ("enable_mqtt", enable_mqtt),
        ("disable_mqtt", disable_mqtt),
        ("snapshot_all", snapshot_all),
        ("start_application", start_application),
        ("stop_application", stop_application),
        ("restart_application", restart_application),
        ("set_view_area_geometry", set_view_area_geometry),
        ("reset_view_area_geometry", reset_view_area_geometry),
        ("play_audio_clip", play_audio_clip),
        ("stop_audio_clip", stop_audio_clip),
        ("delete_audio_clip", delete_audio_clip),
        ("reboot_camera", reboot_camera),
    ]
}

/// Render a numeric argument the way the client sent it (`10` or `10.5`).
fn num(args: &Args, key: &str) -> String {
    args.value(key).map(Value::to_string).unwrap_or_default()
}

// ---- PTZ ----

fn ptz_move(args: &Args) -> anyhow::Result<Vec<Out>> {
    let (pan, tilt, zoom) = (args.f64("pan")?, args.f64("tilt")?, args.i64("zoom")?);
    let cam = args.camera()?;
    ptz::goto(
        &cam.client,
        ptz::GotoParams {
            pan: Some(pan),
            tilt: Some(tilt),
            zoom: Some(zoom as i32),
            rpan: None,
            rtilt: None,
            rzoom: None,
            speed: None,
            camera: None,
        },
    )?;
    Ok(text_out(format!(
        "Moved {} to pan={}, tilt={}, zoom={}",
        cam.id,
        num(args, "pan"),
        num(args, "tilt"),
        num(args, "zoom")
    )))
}

fn ptz_relative(args: &Args) -> anyhow::Result<Vec<Out>> {
    let rpan = args.opt_f64("rpan")?.unwrap_or(0.0);
    let rtilt = args.opt_f64("rtilt")?.unwrap_or(0.0);
    let rzoom = args.opt_i64("rzoom")?.unwrap_or(0);
    let cam = args.camera()?;
    ptz::goto(
        &cam.client,
        ptz::GotoParams {
            pan: None,
            tilt: None,
            zoom: None,
            rpan: Some(rpan),
            rtilt: Some(rtilt),
            rzoom: Some(rzoom as i32),
            speed: None,
            camera: None,
        },
    )?;
    Ok(text_out(format!("Moved {} by relative offset", cam.id)))
}

fn ptz_home(args: &Args) -> anyhow::Result<Vec<Out>> {
    let cam = args.camera()?;
    ptz::move_direction(&cam.client, "home", None)?;
    Ok(text_out(format!("Sent {} to home position", cam.id)))
}

fn ptz_preset(args: &Args) -> anyhow::Result<Vec<Out>> {
    let preset = args.str("preset_name")?;
    let cam = args.camera()?;
    ptz::goto_preset(&cam.client, preset, None)?;
    Ok(text_out(format!("Sent {} to preset '{}'", cam.id, preset)))
}

// ---- I/O and lights ----

fn set_io_port(args: &Args) -> anyhow::Result<Vec<Out>> {
    let (port, state) = (args.str("port")?, args.str("state")?);
    let cam = args.camera()?;
    io::set_port_state(&cam.client, port, state)?;
    Ok(text_out(format!("Set port {} on {} to '{}'", port, cam.id, state)))
}

fn toggle_light(args: &Args) -> anyhow::Result<Vec<Out>> {
    let (light, on) = (args.str("light_id")?, args.bool("on")?);
    let cam = args.camera()?;
    lightcontrol::set_active(&cam.client, light, on)?;
    let verb = if on { "Activated" } else { "Deactivated" };
    Ok(text_out(format!("{} light '{}' on {}", verb, light, cam.id)))
}

// ---- overlays, masks, view areas, streams ----

fn add_overlay(args: &Args) -> anyhow::Result<Vec<Out>> {
    let text = args.str("text")?;
    let position = args.opt_str("position")?.unwrap_or("topLeft");
    let color = args.opt_str("text_color")?.unwrap_or("white");
    let background = args.opt_str("text_bg_color")?.unwrap_or("transparent");
    let cam = args.camera()?;
    let resp = overlay::add_text(&cam.client, 1, text, Some(position), None, Some(color), Some(background))?;
    let identity = resp.pointer("/data/identity").cloned().unwrap_or(Value::Null);
    Ok(text_out(format!("Added text overlay on {} (identity={})", cam.id, identity)))
}

fn remove_overlay(args: &Args) -> anyhow::Result<Vec<Out>> {
    let identity = u32::try_from(args.i64("identity")?).context("identity must be a positive integer")?;
    let cam = args.camera()?;
    overlay::remove(&cam.client, identity)?;
    Ok(text_out(format!("Removed overlay {} from {}", identity, cam.id)))
}

fn add_privacy_mask(args: &Args) -> anyhow::Result<Vec<Out>> {
    let name = args.str("name")?;
    let polygon = args.str("polygon")?;
    let display = (args.opt_i64("display_width")?.unwrap_or(1920), args.opt_i64("display_height")?.unwrap_or(1080));
    let cam = args.camera()?;
    let mut sensor_polygon = polygon.to_string();
    if cam.mask_rotation != 0 || cam.mask_mirror {
        let sensor = privacymask::sensor_resolution(&cam.client);
        sensor_polygon = privacymask::display_to_sensor(polygon, cam.mask_rotation, cam.mask_mirror, sensor, display)?;
    }
    privacymask::add_mask(&cam.client, name, &sensor_polygon)?;
    let note = if cam.mask_rotation != 0 { format!(" (auto-corrected for {}° rotation)", cam.mask_rotation) } else { String::new() };
    Ok(text_out(format!("Added privacy mask '{}' on {}{}", name, cam.id, note)))
}

fn remove_privacy_mask(args: &Args) -> anyhow::Result<Vec<Out>> {
    let name = args.str("name")?;
    let cam = args.camera()?;
    privacymask::remove_mask(&cam.client, name)?;
    Ok(text_out(format!("Removed privacy mask '{}' from {}", name, cam.id)))
}

fn set_view_area_geometry(args: &Args) -> anyhow::Result<Vec<Out>> {
    let id = args.i64("view_area_id")?;
    let (ho, vo) = (args.i64("horizontal_offset")?, args.i64("vertical_offset")?);
    let (hs, vs) = (args.i64("horizontal_size")?, args.i64("vertical_size")?);
    let cam = args.camera()?;
    Ok(json_out(&viewarea::configure_geometry(&cam.client, id, ho, vo, hs, vs)?))
}

fn reset_view_area_geometry(args: &Args) -> anyhow::Result<Vec<Out>> {
    let id = args.i64("view_area_id")?;
    let cam = args.camera()?;
    Ok(json_out(&viewarea::reset_geometry(&cam.client, id)?))
}

fn create_stream_profile(args: &Args) -> anyhow::Result<Vec<Out>> {
    let name = args.str("name")?;
    let parameters = args.str("parameters")?;
    let description = args.opt_str("description")?.unwrap_or("");
    let cam = args.camera()?;
    streamprofile::create_profile(&cam.client, name, parameters, description)?;
    Ok(text_out(format!("Created stream profile '{}' on {}", name, cam.id)))
}

fn remove_stream_profile(args: &Args) -> anyhow::Result<Vec<Out>> {
    let name = args.str("name")?;
    let cam = args.camera()?;
    streamprofile::remove_profile(&cam.client, name)?;
    Ok(text_out(format!("Removed stream profile '{}' from {}", name, cam.id)))
}

// ---- configuration ----

fn set_motion_config(args: &Args) -> anyhow::Result<Vec<Out>> {
    let cameras = args.value("cameras").ok_or_else(|| anyhow!("cameras is required"))?;
    let profiles = args.value("profiles").ok_or_else(|| anyhow!("profiles is required"))?;
    let cam = args.camera()?;
    vmd::set_configuration(&cam.client, cameras, profiles)?;
    Ok(text_out(format!("Updated motion detection configuration on {}", cam.id)))
}

fn set_timezone(args: &Args) -> anyhow::Result<Vec<Out>> {
    let tz = args.str("timezone")?;
    let cam = args.camera()?;
    time::set_timezone(&cam.client, tz)?;
    Ok(text_out(format!("Set timezone on {} to '{}'", cam.id, tz)))
}

fn set_ntp_config(args: &Args) -> anyhow::Result<Vec<Out>> {
    let enabled = args.opt_bool("enabled")?;
    let source = args.opt_str("servers_source")?;
    let servers: Option<Vec<String>> = args
        .value("static_servers")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect());
    let cam = args.camera()?;
    time::set_ntp_config(&cam.client, enabled, source, servers.as_deref())?;
    Ok(text_out(format!("Updated NTP configuration on {}", cam.id)))
}

fn set_daynight_config(args: &Args) -> anyhow::Result<Vec<Out>> {
    let channel = args.opt_i64("channel")?.unwrap_or(0);
    let settings: Map<String, Value> =
        args.0.iter().filter(|(k, _)| *k != "camera_id" && *k != "channel").map(|(k, v)| (k.clone(), v.clone())).collect();
    let cam = args.camera()?;
    daynight::set_configuration(&cam.client, channel, &settings)?;
    Ok(text_out(format!("Updated day/night configuration on {}", cam.id)))
}

fn set_geolocation(args: &Args) -> anyhow::Result<Vec<Out>> {
    let (lat, lng, heading) = (args.opt_f64("lat")?, args.opt_f64("lng")?, args.opt_f64("heading")?);
    let text = args.opt_str("text")?;
    let cam = args.camera()?;
    geolocation::set_location(&cam.client, lat, lng, heading, text)?;
    Ok(text_out(format!("Updated geolocation on {}", cam.id)))
}

fn set_capture_mode(args: &Args) -> anyhow::Result<Vec<Out>> {
    let channel = args.opt_i64("channel")?.unwrap_or(0);
    let mode = args.i64("capture_mode_id")?;
    let cam = args.camera()?;
    capturemode::set_capture_mode(&cam.client, channel, mode)?;
    Ok(text_out(format!("Capture mode set to {} on {} (channel {}). Camera reboot required.", mode, cam.id, channel)))
}

fn set_audio_settings(args: &Args) -> anyhow::Result<Vec<Out>> {
    let devices = args.value("devices").ok_or_else(|| anyhow!("devices is required"))?;
    let cam = args.camera()?;
    audio::set_devices_settings(&cam.client, devices)?;
    Ok(text_out(format!("Updated audio settings on {}", cam.id)))
}

fn set_analytics_producers(args: &Args) -> anyhow::Result<Vec<Out>> {
    let producers = args.value("producers").ok_or_else(|| anyhow!("producers is required"))?;
    let cam = args.camera()?;
    analyticsmetadata::set_enabled_producers(&cam.client, producers)?;
    Ok(text_out(format!("Updated analytics producers on {}", cam.id)))
}

// ---- MQTT ----

fn configure_mqtt(args: &Args) -> anyhow::Result<Vec<Out>> {
    let host = args.str("host")?;
    let port = args.opt_i64("port")?.unwrap_or(1883);
    let protocol = args.opt_str("protocol")?.unwrap_or("tcp");
    let cam = args.camera()?;
    let mut params = json!({
        "server": {"protocol": protocol, "host": host, "port": port},
        "keepAliveInterval": 60,
        "cleanSession": true,
        "autoReconnect": true,
    });
    for (arg, field) in [("client_id", "clientId"), ("username", "username"), ("password", "password")] {
        if let Some(v) = args.opt_str(arg)?.filter(|v| !v.is_empty()) {
            params[field] = json!(v);
        }
    }
    mqtt::configure_client(&cam.client, &params)?;
    Ok(text_out(format!("Configured MQTT on {} → {}:{}", cam.id, host, port)))
}

fn enable_mqtt(args: &Args) -> anyhow::Result<Vec<Out>> {
    let cam = args.camera()?;
    mqtt::activate_client(&cam.client)?;
    Ok(text_out(format!("MQTT enabled on {}", cam.id)))
}

fn disable_mqtt(args: &Args) -> anyhow::Result<Vec<Out>> {
    let cam = args.camera()?;
    mqtt::deactivate_client(&cam.client)?;
    Ok(text_out(format!("MQTT disabled on {}", cam.id)))
}

// ---- applications ----

fn app_action(args: &Args, action: &str, past: &str) -> anyhow::Result<Vec<Out>> {
    let package = args.str("package_name")?;
    let cam = args.camera()?;
    applications::control(&cam.client, action, package)?;
    Ok(text_out(format!("{} '{}' on {}", past, package, cam.id)))
}

fn start_application(args: &Args) -> anyhow::Result<Vec<Out>> {
    app_action(args, "start", "Started")
}

fn stop_application(args: &Args) -> anyhow::Result<Vec<Out>> {
    app_action(args, "stop", "Stopped")
}

fn restart_application(args: &Args) -> anyhow::Result<Vec<Out>> {
    app_action(args, "restart", "Restarted")
}

// ---- audio clips ----

fn play_audio_clip(args: &Args) -> anyhow::Result<Vec<Out>> {
    let which = args.str("name_or_id")?;
    let cam = args.camera()?;
    let id = audio_clip::play_clip(&cam.client, which)?;
    Ok(json_out(&json!({"clip_id": id, "status": "playing"})))
}

fn stop_audio_clip(args: &Args) -> anyhow::Result<Vec<Out>> {
    let cam = args.camera()?;
    audio_clip::stop_clips(&cam.client)?;
    Ok(json_out(&json!({"status": "stopped"})))
}

fn delete_audio_clip(args: &Args) -> anyhow::Result<Vec<Out>> {
    let which = args.str("name_or_id")?;
    let cam = args.camera()?;
    let id = audio_clip::delete_clip(&cam.client, which)?;
    Ok(json_out(&json!({"clip_id": id, "status": "deleted"})))
}

// ---- guard tours, siren, Clear View ----

fn start_guard_tour(args: &Args) -> anyhow::Result<Vec<Out>> {
    let tour = args.str("tour_id")?;
    let cam = args.camera()?;
    guardtour::set_running(&cam.client, tour, true)?;
    Ok(text_out(format!("Started guard tour {} on {}", tour, cam.id)))
}

fn stop_guard_tour(args: &Args) -> anyhow::Result<Vec<Out>> {
    let tour = args.str("tour_id")?;
    let cam = args.camera()?;
    guardtour::set_running(&cam.client, tour, false)?;
    Ok(text_out(format!("Stopped guard tour {} on {}", tour, cam.id)))
}

/// The `start` parameters for `activate_siren`: a saved profile, or the siren
/// and light settings given, each with the duration if one was set.
fn siren_params(args: &Args) -> anyhow::Result<Value> {
    if let Some(profile) = args.opt_str("profile")?.filter(|p| !p.is_empty()) {
        return Ok(json!({"profile": profile}));
    }
    let duration = args.opt_i64("duration")?.filter(|d| *d != 0);
    let with_duration = |mut cfg: Value| {
        if let Some(d) = duration {
            cfg["duration"] = json!({"unit": "seconds", "value": d});
        }
        cfg
    };
    let mut params = Map::new();
    if let Some(pattern) = args.opt_str("siren_pattern")?.filter(|p| !p.is_empty()) {
        let mut cfg = json!({"pattern": pattern});
        if let Some(i) = args.opt_i64("siren_intensity")?.filter(|i| *i != 0) {
            cfg["intensity"] = json!(i);
        }
        params.insert("siren".into(), with_duration(cfg));
    }
    if let Some(pattern) = args.opt_str("light_pattern")?.filter(|p| !p.is_empty()) {
        let mut cfg = json!({"pattern": pattern, "speed": 1});
        if let Some(colors) = args.value("light_colors").filter(|c| c.as_array().is_some_and(|a| !a.is_empty())) {
            cfg["colors"] = colors.clone();
        }
        if let Some(i) = args.opt_i64("light_intensity")?.filter(|i| *i != 0) {
            cfg["intensity"] = json!(i);
        }
        params.insert("light".into(), with_duration(cfg));
    }
    Ok(Value::Object(params))
}

fn activate_siren(args: &Args) -> anyhow::Result<Vec<Out>> {
    let params = siren_params(args)?;
    let cam = args.camera()?;
    let result = siren::start(&cam.client, &params)?;
    Ok(text_out(format!("Activated siren/light on {}: {}", cam.id, result)))
}

fn stop_siren(args: &Args) -> anyhow::Result<Vec<Out>> {
    let cam = args.camera()?;
    siren::stop(&cam.client)?;
    Ok(text_out(format!("Stopped siren and light on {}", cam.id)))
}

fn start_clear_view(args: &Args) -> anyhow::Result<Vec<Out>> {
    let service = args.opt_i64("service_id")?.unwrap_or(0);
    let duration = args.opt_i64("duration")?.filter(|d| *d != 0);
    let cam = args.camera()?;
    clearview::start(&cam.client, service, duration)?;
    let tail = duration.map(|d| format!(" (duration={}s)", d)).unwrap_or_default();
    Ok(text_out(format!("Started Clear View service {} on {}{}", service, cam.id, tail)))
}

fn stop_clear_view(args: &Args) -> anyhow::Result<Vec<Out>> {
    let service = args.opt_i64("service_id")?.unwrap_or(0);
    let cam = args.camera()?;
    clearview::stop(&cam.client, service)?;
    Ok(text_out(format!("Stopped Clear View service {} on {}", service, cam.id)))
}

// ---- events, snapshots, recordings, reboot ----

fn poll_events(args: &Args) -> anyhow::Result<Vec<Out>> {
    let seconds = args.opt_f64("duration")?.unwrap_or(5.0).clamp(1.0, 30.0);
    let topic = args.opt_str("topic_filter")?.filter(|t| !t.is_empty());
    let cam = args.camera()?;
    let collected = events::poll_events(&cam.client, &cam.creds, &cam.host, topic, seconds)?;
    Ok(json_out(&json!({"events_collected": collected.len(), "events": collected})))
}

fn snapshot_all(args: &Args) -> anyhow::Result<Vec<Out>> {
    let resolution = args.opt_str("resolution")?.unwrap_or("640x480").to_string();
    let cameras = camera::list()?;
    let ids: Vec<String> = cameras.as_array().into_iter().flatten().filter_map(|c| c["id"].as_str().map(String::from)).collect();
    if ids.is_empty() {
        return Ok(text_out("No cameras configured"));
    }
    let shots: Vec<Vec<Out>> = std::thread::scope(|scope| {
        let handles: Vec<_> = ids
            .iter()
            .map(|id| {
                let resolution = resolution.as_str();
                scope.spawn(move || {
                    let jpeg = camera::open(id).and_then(|cam| {
                        cam.client.get_bytes("/axis-cgi/jpg/image.cgi", &[("resolution", resolution), ("compression", "20")])
                    });
                    match jpeg {
                        Ok(bytes) => vec![
                            Out::Text(format!("--- {id} ({id}) ---")),
                            Out::Image(base64::engine::general_purpose::STANDARD.encode(bytes), "image/jpeg"),
                        ],
                        Err(e) => vec![Out::Text(format!("--- {id} ({id}): Error: {e} ---"))],
                    }
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or_default()).collect()
    });
    Ok(shots.into_iter().flatten().collect())
}

/// Where `export_recording` writes: `$VAPX_MCP_EXPORTS_DIR` (or the Python
/// server's `$VAPIX_EXPORTS_DIR`), else `exports` under the user's data directory.
fn exports_dir() -> PathBuf {
    std::env::var_os("VAPX_MCP_EXPORTS_DIR")
        .or_else(|| std::env::var_os("VAPIX_EXPORTS_DIR"))
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("vapx").join("exports"))
}

/// A file name that cannot leave the exports directory.
fn safe_filename(name: &str) -> anyhow::Result<&str> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', '\0']) {
        bail!("filename must be a plain file name without directories");
    }
    Ok(name)
}

fn export_recording(args: &Args) -> anyhow::Result<Vec<Out>> {
    let recording = args.str("recording_id")?;
    let disk = args.str("disk_id")?;
    let (start, stop) = (args.opt_str("start_time")?, args.opt_str("stop_time")?);
    let default_name = format!("{recording}.mkv");
    let filename = safe_filename(args.opt_str("filename")?.filter(|f| !f.is_empty()).unwrap_or(&default_name))?;
    let cam = args.camera()?;
    let bytes = storage::export_recording(&cam.client, recording, disk, start, stop)?;
    let dir = exports_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let path = dir.join(filename);
    std::fs::write(&path, &bytes).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(json_out(&json!({"path": path.display().to_string(), "size_bytes": bytes.len()})))
}

fn reboot_camera(args: &Args) -> anyhow::Result<Vec<Out>> {
    let cam = args.camera()?;
    let mut result = system::reboot(&cam.client)?;
    result["camera"] = json!(cam.id);
    result["note"] = json!("Camera will be unreachable for ~30-60 seconds.");
    Ok(json_out(&result))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: Value) -> Args {
        Args(v.as_object().unwrap().clone())
    }

    #[test]
    fn filenames_cannot_leave_the_exports_directory() {
        assert!(safe_filename("clip.mkv").is_ok());
        for bad in ["", ".", "..", "../x.mkv", "a/b.mkv", "a\\b.mkv"] {
            assert!(safe_filename(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn a_profile_overrides_the_other_siren_settings() {
        let p = siren_params(&args(json!({"profile": "alarm", "siren_pattern": "x"}))).unwrap();
        assert_eq!(p, json!({"profile": "alarm"}));
    }

    #[test]
    fn siren_settings_carry_the_duration_and_light_defaults() {
        let p = siren_params(&args(json!({
            "siren_pattern": "wail", "siren_intensity": 7, "light_pattern": "flash",
            "light_colors": ["red"], "duration": 5
        })))
        .unwrap();
        assert_eq!(p["siren"], json!({"pattern": "wail", "intensity": 7, "duration": {"unit": "seconds", "value": 5}}));
        assert_eq!(
            p["light"],
            json!({"pattern": "flash", "speed": 1, "colors": ["red"], "duration": {"unit": "seconds", "value": 5}})
        );
        assert_eq!(siren_params(&args(json!({}))).unwrap(), json!({}));
    }
}
