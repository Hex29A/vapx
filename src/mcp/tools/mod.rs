//! Tool registry: names, descriptions and input schemas come from `tools.json`
//! (the Python server's, unchanged); a tool is offered once a handler is
//! registered for its name.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, bail};
use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde::Deserialize;
use serde_json::{Map, Value};

use super::camera::{self, Cam};

mod read;
mod write;

/// One piece of a tool result.
pub enum Out {
    Text(String),
    /// Base64 data and its MIME type.
    Image(String, &'static str),
}

/// Pretty JSON text, like the Python server's `_text_result` for non-strings.
pub fn json_out(v: &Value) -> Vec<Out> {
    vec![Out::Text(serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string()))]
}

pub fn text_out(s: impl Into<String>) -> Vec<Out> {
    vec![Out::Text(s.into())]
}

/// Tool arguments with typed accessors that name the offending argument.
pub struct Args(pub Map<String, Value>);

impl Args {
    pub fn str(&self, key: &str) -> anyhow::Result<&str> {
        self.opt_str(key)?.ok_or_else(|| anyhow!("{} is required", key))
    }

    pub fn opt_str(&self, key: &str) -> anyhow::Result<Option<&str>> {
        match self.0.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            Some(_) => bail!("{} must be a string", key),
        }
    }

    pub fn opt_i64(&self, key: &str) -> anyhow::Result<Option<i64>> {
        match self.0.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v.as_i64().map(Some).ok_or_else(|| anyhow!("{} must be an integer", key)),
        }
    }

    pub fn opt_f64(&self, key: &str) -> anyhow::Result<Option<f64>> {
        match self.0.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v.as_f64().map(Some).ok_or_else(|| anyhow!("{} must be a number", key)),
        }
    }

    pub fn f64(&self, key: &str) -> anyhow::Result<f64> {
        self.opt_f64(key)?.ok_or_else(|| anyhow!("{} is required", key))
    }

    pub fn i64(&self, key: &str) -> anyhow::Result<i64> {
        self.opt_i64(key)?.ok_or_else(|| anyhow!("{} is required", key))
    }

    pub fn opt_bool(&self, key: &str) -> anyhow::Result<Option<bool>> {
        match self.0.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(_) => bail!("{} must be a boolean", key),
        }
    }

    pub fn bool(&self, key: &str) -> anyhow::Result<bool> {
        self.opt_bool(key)?.ok_or_else(|| anyhow!("{} is required", key))
    }

    /// Any JSON value, `None` when absent or null.
    pub fn value(&self, key: &str) -> Option<&Value> {
        self.0.get(key).filter(|v| !v.is_null())
    }

    /// The camera named by `camera_id`.
    pub fn camera(&self) -> anyhow::Result<Cam> {
        let id = self.opt_str("camera_id")?.filter(|s| !s.is_empty());
        camera::open(id.ok_or_else(|| anyhow!("camera_id is required"))?)
    }
}

pub type Handler = fn(&Args) -> anyhow::Result<Vec<Out>>;

#[derive(Deserialize)]
struct Def {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: JsonObject,
}

/// Every tool the Python server exposed, with its schema.
fn definitions() -> Vec<Def> {
    serde_json::from_str(include_str!("../tools.json")).expect("tools.json is valid")
}

pub struct Registry {
    tools: Vec<Tool>,
    handlers: HashMap<&'static str, Handler>,
    schemas: HashMap<String, Arc<JsonObject>>,
}

/// Tools that change or interrupt a camera in a way that is hard to undo.
/// Clients that ask before running destructive tools will ask for these.
const DESTRUCTIVE: &[&str] = &[
    "reboot_camera", "delete_audio_clip", "remove_overlay", "remove_privacy_mask", "remove_stream_profile",
    "stop_application", "restart_application", "set_capture_mode", "set_motion_config",
    "set_analytics_producers", "set_audio_settings", "export_recording",
];

/// Warnings added to a tool's description, so a client (and the person behind
/// it) sees what the call does before it is made.
fn notice(name: &str) -> Option<&'static str> {
    Some(match name {
        "reboot_camera" => "Disruptive: the camera stops recording and streaming while it restarts. Confirm with the user first.",
        "activate_siren" => "Disruptive: sounds a siren and flashes a strobe at the camera's location. Confirm with the user first.",
        "set_io_port" => "Switches a physical output (relay): this can open doors or start equipment. Confirm with the user first.",
        "stop_application" | "restart_application" => "Disruptive: the application stops working (and loses its state) while it is stopped. Confirm with the user first.",
        "delete_audio_clip" | "remove_overlay" | "remove_privacy_mask" | "remove_stream_profile" => "Destructive: the item is deleted and cannot be restored by this server. Confirm with the user first.",
        "set_capture_mode" => "Disruptive: the camera must be rebooted for the new mode to apply.",
        "export_recording" => "Writes a file on the machine running this server.",
        _ => return None,
    })
}

fn annotations(name: &str, read_only: bool) -> ToolAnnotations {
    let a = ToolAnnotations::new().read_only(read_only).open_world(false);
    if read_only {
        a.idempotent(true)
    } else {
        a.destructive(DESTRUCTIVE.contains(&name))
    }
}

impl Registry {
    pub fn new() -> Self {
        let read: Vec<(&'static str, Handler)> = read::handlers();
        let write: Vec<(&'static str, Handler)> = write::handlers();
        let read_only: Vec<&str> = read.iter().map(|(n, _)| *n).collect();
        let handlers: HashMap<&'static str, Handler> = read.into_iter().chain(write).collect();
        let mut tools = Vec::new();
        let mut schemas = HashMap::new();
        for d in definitions().into_iter().filter(|d| handlers.contains_key(d.name.as_str())) {
            let description = match notice(&d.name) {
                Some(n) if !d.description.contains(n) => format!("{} {}", d.description, n),
                _ => d.description.clone(),
            };
            let schema = Arc::new(d.input_schema);
            schemas.insert(d.name.clone(), schema.clone());
            let ro = read_only.contains(&d.name.as_str());
            tools.push(Tool::new(d.name.clone(), description, schema).with_annotations(annotations(&d.name, ro)));
        }
        Self { tools, handlers, schemas }
    }

    pub fn list(&self) -> Vec<Tool> {
        self.tools.clone()
    }

    pub fn handler(&self, name: &str) -> Option<Handler> {
        self.handlers.get(name).copied()
    }

    /// Check `args` against the tool's input schema.
    pub fn validate(&self, name: &str, args: &Args) -> anyhow::Result<()> {
        match self.schemas.get(name) {
            Some(schema) => validate_args(schema, &args.0),
            None => Ok(()),
        }
    }

    #[cfg(test)]
    pub fn names(&self) -> Vec<String> {
        self.tools.iter().map(|t| t.name.to_string()).collect()
    }
}

/// Check arguments against the subset of JSON Schema the tool definitions use:
/// `required`, and per property `type`, `enum`, `minimum`, `maximum`,
/// `maxLength` and the `type` of array items. Unknown arguments are allowed.
pub fn validate_args(schema: &JsonObject, args: &Map<String, Value>) -> anyhow::Result<()> {
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for key in required.iter().filter_map(Value::as_str) {
            if args.get(key).is_none_or(Value::is_null) {
                bail!("{} is required", key);
            }
        }
    }
    let Some(props) = schema.get("properties").and_then(Value::as_object) else { return Ok(()) };
    for (key, spec) in props {
        let Some(value) = args.get(key).filter(|v| !v.is_null()) else { continue };
        check_value(key, spec, value)?;
    }
    Ok(())
}

fn type_ok(ty: &str, v: &Value) -> bool {
    match ty {
        "string" => v.is_string(),
        "boolean" => v.is_boolean(),
        "array" => v.is_array(),
        "object" => v.is_object(),
        "number" => v.is_number(),
        "integer" => v.is_i64() || v.is_u64() || v.as_f64().is_some_and(|f| f.fract() == 0.0),
        _ => true,
    }
}

fn check_value(key: &str, spec: &Value, value: &Value) -> anyhow::Result<()> {
    if let Some(ty) = spec.get("type").and_then(Value::as_str) {
        if !type_ok(ty, value) {
            bail!("{} must be {} {}", key, if ty == "integer" || ty == "array" || ty == "object" { "an" } else { "a" }, ty);
        }
        if ty == "array" {
            if let (Some(item_ty), Some(items)) = (spec.pointer("/items/type").and_then(Value::as_str), value.as_array()) {
                if items.iter().any(|i| !type_ok(item_ty, i)) {
                    bail!("{} must only contain items of type {}", key, item_ty);
                }
            }
        }
    }
    if let Some(allowed) = spec.get("enum").and_then(Value::as_array) {
        if !allowed.contains(value) {
            let list: Vec<String> = allowed.iter().map(|a| a.as_str().map(String::from).unwrap_or_else(|| a.to_string())).collect();
            bail!("{} must be one of: {}", key, list.join(", "));
        }
    }
    if let Some(n) = value.as_f64() {
        if spec.get("minimum").and_then(Value::as_f64).is_some_and(|min| n < min) {
            bail!("{} must be at least {}", key, spec["minimum"]);
        }
        if spec.get("maximum").and_then(Value::as_f64).is_some_and(|max| n > max) {
            bail!("{} must be at most {}", key, spec["maximum"]);
        }
    }
    if let (Some(s), Some(max)) = (value.as_str(), spec.get("maxLength").and_then(Value::as_u64)) {
        if s.chars().count() as u64 > max {
            bail!("{} must be at most {} characters", key, max);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Every handler has a definition, so no tool is offered without a schema.
    #[test]
    fn every_handler_has_a_definition() {
        let defs: Vec<String> = definitions().into_iter().map(|d| d.name).collect();
        for (name, _) in read::handlers() {
            assert!(defs.iter().any(|d| d == name), "no definition for {name}");
        }
    }

    #[test]
    fn registered_tools_have_object_schemas() {
        let reg = Registry::new();
        assert!(!reg.list().is_empty());
        for t in reg.list() {
            assert_eq!(t.input_schema.get("type").and_then(Value::as_str), Some("object"), "{}", t.name);
        }
    }

    /// The read-only tools of the Python server (#57).
    const READ_TOOLS: &[&str] = &[
        "list_cameras", "get_camera_info", "get_snapshot", "status_all", "get_temperature",
        "get_time_info", "get_ntp_status", "get_system_log", "get_audit_log", "get_users",
        "list_applications", "list_stream_profiles", "get_stream_status", "get_io_ports",
        "get_disk_status", "list_recordings", "get_recording_info", "get_daynight_config",
        "get_motion_config", "get_lights", "get_mqtt_config", "list_overlays",
        "list_privacy_masks", "list_view_areas", "get_capture_modes", "get_orientation",
        "get_geolocation", "get_signed_video_status", "get_audio_settings", "list_audio_clips",
        "list_guard_tours", "ptz_status", "get_siren_status", "get_clear_view_info",
        "list_analytics_producers", "check_systemready", "discover_apis",
    ];

    #[test]
    fn all_read_tools_are_registered_with_a_schema() {
        let reg = Registry::new();
        let names = reg.names();
        for want in READ_TOOLS {
            assert!(names.iter().any(|n| n == want), "missing tool {want}");
        }
        for t in reg.list() {
            if READ_TOOLS.contains(&t.name.as_ref()) {
                assert!(t.input_schema.contains_key("properties"), "{} has no properties", t.name);
            }
        }
    }

    #[test]
    fn args_report_the_offending_argument() {
        let args = Args(serde_json::from_str(r#"{"camera_id": 5, "lines": "x"}"#).unwrap());
        assert!(args.opt_str("camera_id").unwrap_err().to_string().contains("camera_id"));
        assert!(args.opt_i64("lines").unwrap_err().to_string().contains("lines"));
        assert!(args.str("missing").unwrap_err().to_string().contains("missing"));
    }
    /// Every tool the Python server exposes (#58). A tool missing here or in
    /// `tools/list` breaks clients that still expect the Python server's set.
    const ALL_TOOLS: &[&str] = &[
        "activate_siren",
        "add_overlay",
        "add_privacy_mask",
        "check_systemready",
        "configure_mqtt",
        "create_stream_profile",
        "delete_audio_clip",
        "disable_mqtt",
        "discover_apis",
        "enable_mqtt",
        "export_recording",
        "get_audio_settings",
        "get_audit_log",
        "get_camera_info",
        "get_capture_modes",
        "get_clear_view_info",
        "get_daynight_config",
        "get_disk_status",
        "get_geolocation",
        "get_io_ports",
        "get_lights",
        "get_motion_config",
        "get_mqtt_config",
        "get_ntp_status",
        "get_orientation",
        "get_recording_info",
        "get_signed_video_status",
        "get_siren_status",
        "get_snapshot",
        "get_stream_status",
        "get_system_log",
        "get_temperature",
        "get_time_info",
        "get_users",
        "list_analytics_producers",
        "list_applications",
        "list_audio_clips",
        "list_cameras",
        "list_guard_tours",
        "list_overlays",
        "list_privacy_masks",
        "list_recordings",
        "list_stream_profiles",
        "list_view_areas",
        "play_audio_clip",
        "poll_events",
        "ptz_home",
        "ptz_move",
        "ptz_preset",
        "ptz_relative",
        "ptz_status",
        "reboot_camera",
        "remove_overlay",
        "remove_privacy_mask",
        "remove_stream_profile",
        "reset_view_area_geometry",
        "restart_application",
        "set_analytics_producers",
        "set_audio_settings",
        "set_capture_mode",
        "set_daynight_config",
        "set_geolocation",
        "set_io_port",
        "set_motion_config",
        "set_ntp_config",
        "set_timezone",
        "set_view_area_geometry",
        "snapshot_all",
        "start_application",
        "start_clear_view",
        "start_guard_tour",
        "status_all",
        "stop_application",
        "stop_audio_clip",
        "stop_clear_view",
        "stop_guard_tour",
        "stop_siren",
        "toggle_light",
    ];

    #[test]
    fn tools_list_has_every_tool_of_the_python_server() {
        let mut got = Registry::new().names();
        got.sort();
        let mut want: Vec<String> = ALL_TOOLS.iter().map(|s| s.to_string()).collect();
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn write_tools_are_marked_and_disruptive_ones_say_so() {
        let reg = Registry::new();
        let tools = reg.list();
        let by_name = |n: &str| tools.iter().find(|t| t.name == n).unwrap();
        assert_eq!(by_name("get_users").annotations.as_ref().unwrap().read_only_hint, Some(true));
        let reboot = by_name("reboot_camera");
        assert_eq!(reboot.annotations.as_ref().unwrap().read_only_hint, Some(false));
        assert_eq!(reboot.annotations.as_ref().unwrap().destructive_hint, Some(true));
        for (name, word) in [("reboot_camera", "Disruptive"), ("activate_siren", "Disruptive"), ("set_io_port", "relay"),
                             ("delete_audio_clip", "Destructive"), ("export_recording", "file")] {
            let d = by_name(name).description.clone().unwrap_or_default();
            assert!(d.contains(word), "{name}: {d}");
        }
    }

    /// A value of the wrong type for a schema property.
    fn wrong_value(spec: &Value) -> Value {
        match spec.get("type").and_then(Value::as_str) {
            Some("string") => json!(7),
            _ => json!("not-the-right-type"),
        }
    }

    /// Argument validation, for every tool: each required argument is
    /// demanded by name, each typed argument rejects a value of the wrong
    /// type, enums and numeric ranges are enforced. No camera is needed: the
    /// check runs before the tool does.
    #[test]
    fn every_tool_validates_its_arguments() {
        let reg = Registry::new();
        for def in definitions() {
            let schema = &def.input_schema;
            let props = schema.get("properties").and_then(Value::as_object).cloned().unwrap_or_default();
            let required: Vec<String> = schema
                .get("required")
                .and_then(Value::as_array)
                .map(|r| r.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            // A complete, valid argument set to start from.
            let valid = |key: &str, spec: &Value| -> Value {
                if let Some(e) = spec.get("enum").and_then(Value::as_array) {
                    return e[0].clone();
                }
                match spec.get("type").and_then(Value::as_str) {
                    Some("string") => json!("x"),
                    Some("integer") => json!(spec.get("minimum").and_then(Value::as_i64).unwrap_or(1)),
                    Some("number") => json!(spec.get("minimum").and_then(Value::as_f64).unwrap_or(1.0)),
                    Some("boolean") => json!(true),
                    Some("array") => json!([]),
                    _ => panic!("{}: no sample for {key}", def.name),
                }
            };
            let full: Map<String, Value> = required.iter().map(|k| (k.clone(), valid(k, &props[k]))).collect();
            assert!(validate_args(schema, &full).is_ok(), "{}: sample arguments rejected", def.name);

            for key in &required {
                let mut args = full.clone();
                args.remove(key);
                let err = validate_args(schema, &args).unwrap_err().to_string();
                assert!(err.contains(key.as_str()) && err.contains("required"), "{}: {err}", def.name);
            }
            for (key, spec) in &props {
                let mut args = full.clone();
                args.insert(key.clone(), wrong_value(spec));
                let err = validate_args(schema, &args);
                if spec.get("type").is_some() {
                    assert!(err.as_ref().is_err_and(|e| e.to_string().contains(key.as_str())), "{}.{key}: {err:?}", def.name);
                }
                if spec.get("enum").is_some() {
                    args.insert(key.clone(), json!("no-such-value"));
                    assert!(validate_args(schema, &args).unwrap_err().to_string().contains("one of"), "{}.{key}", def.name);
                }
                if let Some(max) = spec.get("maximum").and_then(Value::as_f64) {
                    args.insert(key.clone(), json!(max + 1.0));
                    assert!(validate_args(schema, &args).unwrap_err().to_string().contains("at most"), "{}.{key}", def.name);
                }
                if let Some(min) = spec.get("minimum").and_then(Value::as_f64) {
                    args.insert(key.clone(), json!(min - 1.0));
                    assert!(validate_args(schema, &args).unwrap_err().to_string().contains("at least"), "{}.{key}", def.name);
                }
                if let Some(max) = spec.get("maxLength").and_then(Value::as_u64) {
                    args.insert(key.clone(), json!("x".repeat(max as usize + 1)));
                    assert!(validate_args(schema, &args).unwrap_err().to_string().contains("characters"), "{}.{key}", def.name);
                }
            }
        }
        // The registry applies the same check in front of a handler.
        let bad = Args(serde_json::from_str(r#"{"camera_id": "c", "zoom": 0, "pan": 0, "tilt": 0}"#).unwrap());
        assert!(reg.validate("ptz_move", &bad).unwrap_err().to_string().contains("zoom"));
    }

    #[test]
    fn array_items_are_type_checked() {
        let reg = Registry::new();
        let bad = Args(serde_json::from_str(r#"{"camera_id": "c", "static_servers": ["a", 3]}"#).unwrap());
        assert!(reg.validate("set_ntp_config", &bad).unwrap_err().to_string().contains("static_servers"));
    }
}
