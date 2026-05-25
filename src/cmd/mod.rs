pub mod acap;
pub mod audio;
pub mod audit;
pub mod backup;
pub mod batch;
pub mod cert;
pub mod config;
pub mod daynight;
pub mod diff;
pub mod discover;
pub mod events;
pub mod fw;
pub mod health;
pub mod hw;
pub mod imaging;
pub mod info;
pub mod light;
pub mod log;
pub mod net;
pub mod overlay;
pub mod param;
pub mod pass;
pub mod ptz;
pub mod rule;
pub mod snap;
pub mod storage;
pub mod stream;
pub mod temp;
pub mod template;
pub mod time;
pub mod user;
pub mod vmd;
pub mod watch;

/// Parse param.cgi key=value text into a JSON map.
pub fn param_to_json(text: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.to_string(), serde_json::Value::String(v.to_string()));
        }
    }
    map
}
