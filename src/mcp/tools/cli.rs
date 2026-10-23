//! Tools for features the Python server never had, taken from the CLI:
//! firmware, parameters, imaging, fleet health, audits, backups, diffs,
//! certificates, stream URLs, self test and ZipStream.
//!
//! Left out on purpose, because they can lock you out of a camera:
//! factory default, passwords, user accounts, SSH accounts, network settings
//! and enrolment. They stay CLI-only.

use std::path::Path;
use std::time::Instant;

use anyhow::{anyhow, bail, Context};
use serde_json::{json, Map, Value};

use super::write::{exports_dir, safe_filename};
use super::{json_out, Args, Handler, Out};
use crate::cmd::{audit, fw, health, param_to_json};
use crate::mcp::camera::{self, Cam};
use crate::vapix::{certs, data_of, disk, firmware, image, params, selftest, streamurl, zipstream};

/// Tools in this file that do not change anything.
pub const READ_ONLY: &[&str] = &[
    "firmware_status", "firmware_check", "param_get", "imaging_get", "fleet_health", "security_audit",
    "diff_params", "cert_list", "stream_urls", "zipstream_status", "disk_status",
];

pub fn handlers() -> Vec<(&'static str, Handler)> {
    vec![
        ("firmware_status", firmware_status),
        ("firmware_check", firmware_check),
        ("firmware_upgrade", firmware_upgrade),
        ("param_get", param_get),
        ("param_set", param_set),
        ("imaging_get", imaging_get),
        ("imaging_set", imaging_set),
        ("fleet_health", fleet_health),
        ("security_audit", security_audit),
        ("backup_params", backup_params),
        ("diff_params", diff_params),
        ("cert_list", cert_list),
        ("stream_urls", stream_urls),
        ("selftest", run_selftest),
        ("zipstream_status", zipstream_status),
        ("disk_status", disk_status),
        ("disk_format", disk_format),
        ("disk_encrypt", disk_encrypt),
    ]
}

fn on_camera(args: &Args, f: impl FnOnce(&Cam) -> anyhow::Result<Value>) -> anyhow::Result<Vec<Out>> {
    let cam = args.camera()?;
    Ok(json_out(&f(&cam)?))
}

// ---- firmware ----

fn firmware_status(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| Ok(data_of(firmware::status(&c.client)?)))
}

fn firmware_check(args: &Args) -> anyhow::Result<Vec<Out>> {
    let expected = args.opt_str("expected_version")?.filter(|v| !v.is_empty());
    let other = args.opt_str("compare_to")?.filter(|v| !v.is_empty());
    let cam = args.camera()?;
    let current = firmware::active_version(&cam.client)?;
    let result = match (expected, other) {
        (Some(want), None) => json!({"host": cam.id, "current": current, "expected": want, "matches": current == want}),
        (None, Some(other)) => {
            let version_b = firmware::active_version(&camera::open(other)?.client)?;
            json!({"host_a": cam.id, "version_a": current, "host_b": other, "version_b": version_b,
                   "matches": current == version_b})
        }
        _ => bail!("give either expected_version or compare_to"),
    };
    Ok(json_out(&result))
}

/// Largest firmware file accepted (current Axis images are a few hundred MB).
const MAX_FIRMWARE_BYTES: u64 = 1 << 30;

/// The firmware file to upload, checked before anything is read or sent.
fn firmware_file(path: &str) -> anyhow::Result<&Path> {
    let p = Path::new(path);
    if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("bin")) {
        bail!("file_path must be a firmware .bin file");
    }
    let meta = std::fs::metadata(p).with_context(|| format!("Firmware file not found: {path}"))?;
    if !meta.is_file() {
        bail!("file_path must be a regular file");
    }
    if meta.len() > MAX_FIRMWARE_BYTES {
        bail!("Firmware file is larger than {} MB", MAX_FIRMWARE_BYTES >> 20);
    }
    Ok(p)
}

fn firmware_upgrade(args: &Args) -> anyhow::Result<Vec<Out>> {
    if args.opt_bool("confirm")? != Some(true) {
        bail!("confirm must be true: the upgrade reboots the camera");
    }
    let file = firmware_file(args.str("file_path")?)?;
    let wait = args.opt_bool("wait")?.unwrap_or(false);
    let auto_commit = args.opt_bool("auto_commit")?.unwrap_or(false);
    let wait_timeout = args.opt_i64("wait_timeout")?.unwrap_or(300).max(30) as u64;
    if auto_commit && !wait {
        bail!("auto_commit needs wait");
    }
    let cam = args.camera_for_firmware()?;
    let data = std::fs::read(file).with_context(|| format!("cannot read {}", file.display()))?;
    let old_version = firmware::active_version(&cam.client)?;

    let resp = firmware::upgrade(&cam.client, &data, None, None, None)?;
    let new_version = resp.pointer("/data/firmwareVersion").and_then(Value::as_str).unwrap_or("unknown").to_string();

    let mut committed = false;
    if wait {
        fw::wait_for_reboot(&cam.host, cam.creds.port, cam.creds.clone(), wait_timeout, false)?;
        if auto_commit {
            firmware::commit(&camera::open(&cam.id)?.client)?;
            committed = true;
        }
    }
    Ok(json_out(&json!({
        "previousVersion": old_version,
        "newVersion": new_version,
        "rebooting": !wait,
        "committed": committed,
    })))
}

// ---- parameters ----

fn param_get(args: &Args) -> anyhow::Result<Vec<Out>> {
    let key = args.str("param")?;
    if key != "root" && !key.starts_with("root.") {
        bail!("param must be root or start with root.");
    }
    on_camera(args, |c| {
        let text = params::list(&c.client, Some(key))?;
        let shown: Map<String, Value> =
            param_to_json(&text).into_iter().map(|(k, v)| { let s = params::shown(&k, v.as_str().unwrap_or("")); (k, json!(s)) }).collect();
        Ok(Value::Object(shown))
    })
}

/// Parameter groups `param_set` will not touch: changing them can cut the
/// camera off from the network or from its users.
const PROTECTED_PREFIXES: &[&str] = &["root.Network", "root.Users", "root.Properties"];

const MAX_ASSIGNMENTS: usize = 50;

/// Whether `key` names one explicit, settable parameter.
fn settable_key(key: &str) -> anyhow::Result<()> {
    let plain = key.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !key.starts_with("root.") || !plain || key.ends_with('.') || key.contains("..") || key.len() > 200 {
        bail!("'{}' is not an explicit parameter key (wildcards and groups are not allowed)", key);
    }
    if PROTECTED_PREFIXES.iter().any(|p| key == *p || key.starts_with(&format!("{p}."))) {
        bail!("'{}' is protected: changing it can lock you out of the camera. Use the vapx command line.", key);
    }
    Ok(())
}

/// The text a JSON value is sent as; objects, arrays and control characters are refused.
fn param_value(key: &str, v: &Value) -> anyhow::Result<String> {
    let s = match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => if *b { "yes".into() } else { "no".into() },
        _ => bail!("the value of '{}' must be a string, number or boolean", key),
    };
    if s.chars().any(char::is_control) || s.len() > 1000 {
        bail!("the value of '{}' contains control characters or is too long", key);
    }
    Ok(s)
}

fn param_set(args: &Args) -> anyhow::Result<Vec<Out>> {
    let assignments = args.value("assignments").and_then(Value::as_object).ok_or_else(|| anyhow!("assignments is required"))?;
    if assignments.is_empty() || assignments.len() > MAX_ASSIGNMENTS {
        bail!("assignments must have between 1 and {} entries", MAX_ASSIGNMENTS);
    }
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (k, v) in assignments {
        settable_key(k)?;
        pairs.push((k.clone(), param_value(k, v)?));
    }
    let cam = args.camera()?;
    let refs: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let reply = params::update(&cam.client, &refs)?;
    Ok(json_out(&json!({"updated": pairs.len(), "result": reply.trim()})))
}

const SENSOR_GROUP: &str = "root.ImageSource.I0.Sensor";

fn imaging_get(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| Ok(Value::Object(param_to_json(&image::get_imaging(&c.client)?))))
}

fn imaging_set(args: &Args) -> anyhow::Result<Vec<Out>> {
    let settings = args.value("settings").and_then(Value::as_object).ok_or_else(|| anyhow!("settings is required"))?;
    if settings.is_empty() || settings.len() > MAX_ASSIGNMENTS {
        bail!("settings must have between 1 and {} entries", MAX_ASSIGNMENTS);
    }
    let cam = args.camera()?;
    let current = param_to_json(&image::get_imaging(&cam.client)?);
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (name, v) in settings {
        let key = if name.starts_with("root.") { name.clone() } else { format!("{SENSOR_GROUP}.{name}") };
        if !key.starts_with(&format!("{SENSOR_GROUP}.")) || !current.contains_key(&key) {
            bail!("'{}' is not an image sensor setting on this camera (see imaging_get)", name);
        }
        pairs.push((key.clone(), param_value(&key, v)?));
    }
    let refs: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let reply = params::update(&cam.client, &refs)?;
    Ok(json_out(&json!({"updated": pairs.len(), "result": reply.trim()})))
}

// ---- fleet ----

/// Names of the cameras a tool should visit: `targets` (a group or a comma
/// list), else every enabled camera.
fn target_cameras(targets: Option<&str>) -> anyhow::Result<Vec<String>> {
    let config = crate::config::cameras::load_cameras()?.ok_or_else(|| anyhow!("No cameras.yaml found"))?;
    let mut names = match targets.filter(|t| !t.is_empty()) {
        Some(t) => crate::cmd::resolve_targets(&config, t)?,
        None => config.cameras.iter().filter(|(_, e)| e.enabled).map(|(n, _)| n.clone()).collect(),
    };
    names.sort();
    if names.is_empty() {
        bail!("No cameras matched");
    }
    Ok(names)
}

/// Run `f` for each camera name in parallel; results come back in name order.
fn for_each_camera(names: &[String], f: impl Fn(&str) -> Value + Sync) -> Vec<Value> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = names.iter().map(|n| { let f = &f; scope.spawn(move || f(n)) }).collect();
        handles.into_iter().map(|h| h.join().unwrap_or(Value::Null)).collect()
    })
}

fn fleet_health(args: &Args) -> anyhow::Result<Vec<Out>> {
    let names = target_cameras(args.opt_str("targets")?)?;
    let results = for_each_camera(&names, |name| {
        let start = Instant::now();
        match camera::open(name) {
            Ok(cam) => health::check_client(name, &cam.host, &cam.client, start),
            Err(e) => json!({"camera": name, "status": "unreachable", "error": e.to_string()}),
        }
    });
    Ok(json_out(&json!({"summary": health::summarize(&results), "cameras": results})))
}

fn security_audit(args: &Args) -> anyhow::Result<Vec<Out>> {
    let one = args.opt_str("camera_id")?.filter(|c| !c.is_empty());
    let names = match one {
        Some(id) => vec![id.to_string()],
        None => target_cameras(None)?,
    };
    let audit_one = |name: &str| match camera::open(name) {
        Ok(cam) => audit::audit_report(name, &cam.client, &cam.creds),
        Err(e) => json!({"host": name, "error": e.to_string()}),
    };
    if one.is_some() {
        return Ok(json_out(&audit_one(&names[0])));
    }
    Ok(json_out(&json!({"cameras": for_each_camera(&names, audit_one)})))
}

// ---- backup and diff ----

fn backup_params(args: &Args) -> anyhow::Result<Vec<Out>> {
    let group = args.opt_str("group")?.filter(|g| !g.is_empty());
    let cam = args.camera()?;
    let default_name = format!("{}-params.json", cam.id);
    let filename = safe_filename(args.opt_str("filename")?.filter(|f| !f.is_empty()).unwrap_or(&default_name))?.to_string();
    let map = params::parse(&params::list(&cam.client, group)?);
    let dir = exports_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let path = dir.join(&filename);
    write_private(&path, serde_json::to_string_pretty(&map)?.as_bytes())?;
    Ok(json_out(&json!({"path": path.display().to_string(), "parameters": map.len()})))
}

/// Write a file only its owner can read (parameter dumps hold secrets).
fn write_private(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).with_context(|| format!("cannot write {}", path.display()))?;
    file.write_all(bytes).with_context(|| format!("cannot write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // A file that already existed keeps its old mode otherwise.
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

const MAX_DIFFS: usize = 500;

fn diff_params(args: &Args) -> anyhow::Result<Vec<Out>> {
    let (a, b) = (args.str("camera_a")?, args.str("camera_b")?);
    let group = args.opt_str("group")?.filter(|g| !g.is_empty());
    let (cam_a, cam_b) = (camera::open(a)?, camera::open(b)?);
    let map_a = params::parse(&params::list(&cam_a.client, group)?);
    let map_b = params::parse(&params::list(&cam_b.client, group)?);
    let mut diffs = params::diff(a, b, &map_a, &map_b);
    let total = diffs.len();
    diffs.truncate(MAX_DIFFS);
    for d in &mut diffs {
        if d["param"].as_str().is_some_and(params::is_sensitive) {
            for name in [a, b] {
                if d.get(name).is_some() {
                    d[name] = json!("[redacted]");
                }
            }
        }
    }
    Ok(json_out(&json!({
        "camera_a": a, "camera_b": b, "total_diffs": total, "truncated": total > MAX_DIFFS, "diffs": diffs,
    })))
}

// ---- the rest ----

fn cert_list(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| Ok(data_of(certs::list(&c.client)?)))
}

fn stream_urls(args: &Args) -> anyhow::Result<Vec<Out>> {
    let resolution = args.opt_str("resolution")?.filter(|r| !r.is_empty());
    if let Some(r) = resolution {
        let ok = r.split_once('x').is_some_and(|(w, h)| {
            [w, h].iter().all(|n| !n.is_empty() && n.len() <= 5 && n.chars().all(|c| c.is_ascii_digit()))
        });
        if !ok {
            bail!("resolution must look like 1920x1080");
        }
    }
    let channel = args.opt_i64("channel")?.unwrap_or(1) as u8;
    let fps = args.opt_i64("fps")?.map(|f| f as u8);
    let stream_profile = args.opt_i64("stream_profile")?.unwrap_or(0) as u8;
    let codec = args.opt_str("codec")?;
    let cam = args.camera()?;
    let o = streamurl::StreamOpts { channel, resolution, codec, fps, stream_profile };
    let (https, port) = (cam.creds.https, cam.creds.port);
    Ok(json_out(&json!({
        "rtsp": streamurl::rtsp(&cam.host, None, &o),
        "mjpeg": streamurl::mjpeg(https, &cam.host, port, None, &o),
        "snapshot": streamurl::snapshot(https, &cam.host, port, None, &o),
        "note": "No credentials are included; authenticate with the camera's user and password.",
    })))
}

fn run_selftest(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| Ok(data_of(selftest::run_self_test(&c.client)?)))
}

fn zipstream_status(args: &Args) -> anyhow::Result<Vec<Out>> {
    on_camera(args, |c| zipstream::parse_profiles(&zipstream::list_profiles(&c.client)?))
}

// ---- disks ----

fn disk_status(args: &Args) -> anyhow::Result<Vec<Out>> {
    let only = args.opt_str("disk_id")?.filter(|s| !s.is_empty());
    on_camera(args, |c| disk::status(&c.client, only))
}

fn disk_wait(args: &Args) -> anyhow::Result<disk::Wait> {
    let secs = args.opt_i64("wait_timeout")?.unwrap_or(900).clamp(30, 1800) as u64;
    Ok(disk::Wait { timeout: std::time::Duration::from_secs(secs), ..Default::default() })
}

fn disk_id(args: &Args) -> anyhow::Result<&str> {
    Ok(args.opt_str("disk_id")?.filter(|s| !s.is_empty()).unwrap_or(disk::DEFAULT_DISK))
}

fn disk_format(args: &Args) -> anyhow::Result<Vec<Out>> {
    if args.opt_bool("confirm")? != Some(true) {
        bail!("confirm must be true: formatting erases every recording on the disk");
    }
    let id = disk_id(args)?;
    let filesystem = args.opt_str("filesystem")?.filter(|s| !s.is_empty()).unwrap_or("ext4");
    let wait = disk_wait(args)?;
    on_camera(args, |c| disk::format(&c.client, id, filesystem, wait, |_, _| {}))
}

/// The passphrase of the card comes from the server's own environment, never from a tool argument:
/// arguments pass through the assistant and its logs.
fn server_passphrase() -> anyhow::Result<disk::Secret> {
    let file = std::env::var_os("VAPX_DISK_PASSPHRASE_FILE").map(std::path::PathBuf::from);
    disk::secret_from(file.as_deref(), "VAPX_DISK_PASSPHRASE", "passphrase")
}

fn disk_encrypt(args: &Args) -> anyhow::Result<Vec<Out>> {
    if args.opt_bool("confirm")? != Some(true) {
        bail!("confirm must be true: switching encryption erases every recording on the disk");
    }
    let enable = args.bool("enable")?;
    let id = disk_id(args)?;
    let wait = disk_wait(args)?;
    let passphrase = server_passphrase()?;
    on_camera(args, |c| disk::set_encryption(&c.client, id, enable, &passphrase, wait, |_, _| {}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_explicit_unprotected_keys_can_be_set() {
        assert!(settable_key("root.Image.I0.Appearance.Rotation").is_ok());
        for bad in ["Image.Rotation", "root.", "root.Image.", "root.Image.*", "root.Image.I?", "root.Image..X", "root.A B", "root.a=b"] {
            assert!(settable_key(bad).unwrap_err().to_string().contains("explicit"), "{bad}");
        }
        for bad in ["root.Network.IPv6.Enabled", "root.Network", "root.Users.U0.Name", "root.Properties.API.HTTP.Version"] {
            assert!(settable_key(bad).unwrap_err().to_string().contains("protected"), "{bad}");
        }
        // Only whole path segments are protected.
        assert!(settable_key("root.NetworkTime.Foo").is_ok());
    }

    #[test]
    fn values_are_sent_as_plain_text() {
        assert_eq!(param_value("k", &json!("a b")).unwrap(), "a b");
        assert_eq!(param_value("k", &json!(5)).unwrap(), "5");
        assert_eq!(param_value("k", &json!(true)).unwrap(), "yes");
        assert!(param_value("k", &json!({"a": 1})).is_err());
        assert!(param_value("k", &json!("a\nb")).is_err());
    }

    #[test]
    fn firmware_files_are_checked_before_anything_is_read() {
        assert!(firmware_file("/nonexistent/fw.bin").unwrap_err().to_string().contains("not found"));
        assert!(firmware_file("/etc/passwd").unwrap_err().to_string().contains(".bin"));
        let dir = std::env::temp_dir().join(format!("vapx-fwtest-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("dir.bin")).unwrap();
        assert!(firmware_file(dir.join("dir.bin").to_str().unwrap()).unwrap_err().to_string().contains("regular file"));
        std::fs::write(dir.join("ok.BIN"), b"x").unwrap();
        assert!(firmware_file(dir.join("ok.BIN").to_str().unwrap()).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
