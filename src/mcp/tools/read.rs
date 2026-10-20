//! Read-only tools. Each calls the same `vapix` library code as the CLI.

use anyhow::Context;
use base64::Engine;
use serde_json::{json, Value};

use super::{json_out, text_out, Args, Handler, Out};
use crate::mcp::camera::{self, Cam};
use crate::vapix::{
    analyticsmetadata, applications, audio, audio_clip, capturemode, clearview, daynight, device, discover,
    geolocation, guardtour, io, lightcontrol, mqtt, orientation, overlay, privacymask, ptz, signedvideo,
    siren, storage, streamprofile, streamstatus, system, systemready, temperature, time, users, viewarea,
    vmd,
};

pub fn handlers() -> Vec<(&'static str, Handler)> {
    vec![
        ("list_cameras", list_cameras),
        ("get_camera_info", get_camera_info),
        ("get_snapshot", get_snapshot),
        ("status_all", status_all),
        ("get_temperature", get_temperature),
        ("get_time_info", get_time_info),
        ("get_ntp_status", get_ntp_status),
        ("get_system_log", get_system_log),
        ("get_audit_log", get_audit_log),
        ("get_users", get_users),
        ("list_applications", list_applications),
        ("list_stream_profiles", list_stream_profiles),
        ("get_stream_status", get_stream_status),
        ("get_io_ports", get_io_ports),
        ("get_disk_status", get_disk_status),
        ("list_recordings", list_recordings),
        ("get_recording_info", get_recording_info),
        ("get_daynight_config", get_daynight_config),
        ("get_motion_config", get_motion_config),
        ("get_lights", get_lights),
        ("get_mqtt_config", get_mqtt_config),
        ("list_overlays", list_overlays),
        ("list_privacy_masks", list_privacy_masks),
        ("list_view_areas", list_view_areas),
        ("get_capture_modes", get_capture_modes),
        ("get_orientation", get_orientation),
        ("get_geolocation", get_geolocation),
        ("get_signed_video_status", get_signed_video_status),
        ("get_audio_settings", get_audio_settings),
        ("list_audio_clips", list_audio_clips),
        ("list_guard_tours", list_guard_tours),
        ("ptz_status", ptz_status),
        ("get_siren_status", get_siren_status),
        ("get_clear_view_info", get_clear_view_info),
        ("list_analytics_producers", list_analytics_producers),
        ("check_systemready", check_systemready),
        ("discover_apis", discover_apis),
    ]
}

/// Run `f` against the camera named by `camera_id` and return its JSON as text.
fn on_camera(args: &Args, f: impl FnOnce(&Cam) -> anyhow::Result<Value>) -> anyhow::Result<Vec<Out>> {
    let cam = args.camera()?;
    Ok(json_out(&f(&cam)?))
}

fn list_cameras(_args: &Args) -> anyhow::Result<Vec<Out>> {
    Ok(json_out(&camera::list()?))
}

fn get_camera_info(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |cam| {
        let resp = device::get_all_properties(&cam.client)?;
        let mut props = resp
            .pointer("/data/propertyList")
            .cloned()
            .context("Camera answered without a property list")?;
        if let Some(map) = props.as_object_mut() {
            // WebURL is always the generic vendor page; point at this camera instead.
            map.remove("WebURL");
            map.insert("DeviceURL".into(), json!(format!("http://{}", cam.host)));
        }
        Ok(props)
    })
}

fn get_snapshot(args: &Args) -> anyhow::Result<Vec<Out>> {
    let resolution = args.opt_str("resolution")?.unwrap_or("1920x1080");
    let cam = args.camera()?;
    let jpeg = cam
        .client
        .get_bytes("/axis-cgi/jpg/image.cgi", &[("resolution", resolution), ("compression", "20")])?;
    Ok(vec![Out::Image(base64::engine::general_purpose::STANDARD.encode(jpeg), "image/jpeg")])
}

/// Reachability and identity of every camera, queried in parallel.
fn status_all(_args: &Args) -> anyhow::Result<Vec<Out>> {
    let cameras = camera::list()?;
    let rows = cameras.as_array().cloned().unwrap_or_default();
    let statuses: Vec<Value> = std::thread::scope(|scope| {
        let handles: Vec<_> = rows
            .iter()
            .map(|row| {
                scope.spawn(move || {
                    let mut status = row.clone();
                    let id = row["id"].as_str().unwrap_or_default();
                    let probe = camera::open(id).and_then(|cam| {
                        let resp = device::get_all_properties(&cam.client)?;
                        resp.pointer("/data/propertyList").cloned().context("no property list")
                    });
                    match probe {
                        Ok(props) => {
                            let get = |k: &str| props.get(k).cloned().unwrap_or_else(|| json!("unknown"));
                            status["online"] = json!(true);
                            status["model"] = get("ProdNbr");
                            status["firmware"] = get("Version");
                            status["serial"] = get("SerialNumber");
                        }
                        Err(e) => {
                            status["online"] = json!(false);
                            status["error"] = json!(e.to_string());
                        }
                    }
                    status
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or(Value::Null)).collect()
    });
    Ok(json_out(&Value::Array(statuses)))
}

fn get_temperature(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| Ok(temperature::parse_sensors(&temperature::get_sensors(&c.client)?)))
}

fn get_time_info(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| time::get_date_time_info(&c.client))
}

fn get_ntp_status(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| time::get_ntp_info(&c.client))
}

fn lines_arg(args: &Args) -> anyhow::Result<Option<usize>> {
    Ok(args.opt_i64("lines")?.and_then(|n| usize::try_from(n).ok()))
}

fn get_system_log(args: &Args) -> anyhow::Result<Vec<Out>> {
    let lines = lines_arg(args)?;
    Ok(text_out(system::system_log(&args.camera()?.client, lines)?))
}

fn get_audit_log(args: &Args) -> anyhow::Result<Vec<Out>> {
    let lines = lines_arg(args)?;
    Ok(text_out(system::audit_log(&args.camera()?.client, lines)?))
}

fn get_users(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| users::get_users(&c.client))
}

fn list_applications(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| Ok(serde_json::to_value(applications::list_applications(&c.client)?)?))
}

fn list_stream_profiles(args: &Args) -> anyhow::Result<Vec<Out>> {
    let name = args.opt_str("name")?;
    on_camera(args, |c| streamprofile::list_profiles(&c.client, name))
}

fn get_stream_status(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| streamstatus::get_stream_status(&c.client, &c.creds, &c.host))
}

fn get_io_ports(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| io::get_port_list(&c.client))
}

fn get_disk_status(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| Ok(crate::vapix::data_of(storage::list_disks(&c.client)?)))
}

fn list_recordings(args: &Args) -> anyhow::Result<Vec<Out>> {
    let disk = args.opt_str("disk_id")?;
    let start = args.opt_str("start_time")?;
    let stop = args.opt_str("stop_time")?;
    let max = args
        .opt_i64("max_recordings")?
        .filter(|n| *n > 0)
        .map(|n| n.min(u32::MAX as i64) as u32)
        .unwrap_or(1000);
    on_camera(args, |c| storage::list_recordings_filtered(&c.client, disk, start, stop, max))
}

fn get_recording_info(args: &Args) -> anyhow::Result<Vec<Out>> {
    let recording = args.str("recording_id")?;
    let disk = args.str("disk_id")?;
    let start = args.opt_str("start_time")?;
    let stop = args.opt_str("stop_time")?;
    on_camera(args, |c| storage::get_export_properties(&c.client, recording, disk, start, stop))
}

fn get_daynight_config(args: &Args) -> anyhow::Result<Vec<Out>> {
    let channel = args.opt_i64("channel")?.unwrap_or(0);
    on_camera(args, |c| daynight::get_configuration(&c.client, channel))
}

fn get_motion_config(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| vmd::get_configuration(&c.client))
}

fn get_lights(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| lightcontrol::get_light_information(&c.client))
}

fn get_mqtt_config(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| {
        let client = crate::vapix::data_of(mqtt::get_client_status(&c.client)?);
        let mut events = crate::vapix::data_of(mqtt::get_event_config(&c.client)?);
        let publication = events.get_mut("eventPublicationConfig").map(Value::take).unwrap_or(events);
        Ok(json!({"client": client, "eventPublication": publication}))
    })
}

fn list_overlays(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| Ok(crate::vapix::data_of(overlay::list(&c.client)?)))
}

fn list_privacy_masks(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| privacymask::list_masks(&c.client))
}

fn list_view_areas(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| viewarea::list_normalized(&c.client))
}

fn get_capture_modes(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| capturemode::get_capture_modes(&c.client))
}

fn get_orientation(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| Ok(orientation::get_orientation(&c.client)))
}

fn get_geolocation(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| geolocation::get_location(&c.client))
}

fn get_signed_video_status(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| Ok(crate::vapix::data_of(signedvideo::get_status(&c.client)?)))
}

fn get_audio_settings(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| audio::get_devices_settings(&c.client))
}

fn list_audio_clips(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| audio_clip::list_clips(&c.client))
}

fn list_guard_tours(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| guardtour::list_tours(&c.client))
}

fn ptz_status(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| ptz::position(&c.client))
}

fn get_siren_status(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| siren::get_status(&c.client))
}

fn get_clear_view_info(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| clearview::get_service_info(&c.client))
}

fn list_analytics_producers(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| analyticsmetadata::list_producers(&c.client))
}

fn check_systemready(args: &Args) -> anyhow::Result<Vec<Out>> {
    let timeout = args.opt_i64("timeout")?.unwrap_or(10).clamp(0, 60) as u64;
    on_camera(args, |c| Ok(systemready::query(&c.client, timeout)?.raw))
}

fn discover_apis(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| {
        let mut data = crate::vapix::data_of(discover::get_api_list(&c.client)?);
        Ok(data.get_mut("apiList").map(Value::take).unwrap_or(data))
    })
}
