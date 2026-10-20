//! Tool registry: names, descriptions and input schemas come from `tools.json`
//! (the Python server's, unchanged); a tool is offered once a handler is
//! registered for its name.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, bail};
use rmcp::model::{JsonObject, Tool};
use serde::Deserialize;
use serde_json::{Map, Value};

use super::camera::{self, Cam};

mod read;

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
}

impl Registry {
    pub fn new() -> Self {
        let handlers: HashMap<&'static str, Handler> = read::handlers().into_iter().collect();
        let tools = definitions()
            .into_iter()
            .filter(|d| handlers.contains_key(d.name.as_str()))
            .map(|d| Tool::new(d.name, d.description, Arc::new(d.input_schema)))
            .collect();
        Self { tools, handlers }
    }

    pub fn list(&self) -> Vec<Tool> {
        self.tools.clone()
    }

    pub fn handler(&self, name: &str) -> Option<Handler> {
        self.handlers.get(name).copied()
    }

    #[cfg(test)]
    pub fn names(&self) -> Vec<String> {
        self.tools.iter().map(|t| t.name.to_string()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
