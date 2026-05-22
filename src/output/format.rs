use std::sync::OnceLock;

use serde::Serialize;
use serde_json::Value;

static FILTER_KEYS: OnceLock<Vec<String>> = OnceLock::new();

/// Set the global output filter keys (dot-separated paths like "model" or "firmware.version").
pub fn set_filter(keys: Vec<String>) {
    FILTER_KEYS.set(keys).ok();
}

/// Extract a value at a dot-separated path (e.g., "firmware.version").
fn extract_path(value: &Value, path: &str) -> Option<Value> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = value;
    for part in &parts {
        current = current.get(part)?;
    }
    Some(current.clone())
}

/// Apply the global filter to a data value if one is set.
fn apply_filter(data: &Value) -> Value {
    if let Some(keys) = FILTER_KEYS.get() {
        if !keys.is_empty() {
            if keys.len() == 1 {
                // Single key: output just the value
                return extract_path(data, &keys[0]).unwrap_or(Value::Null);
            }
            // Multiple keys: output as object
            let mut map = serde_json::Map::new();
            for key in keys {
                let val = extract_path(data, key).unwrap_or(Value::Null);
                map.insert(key.clone(), val);
            }
            return Value::Object(map);
        }
    }
    data.clone()
}

/// Output a successful result wrapped in a status envelope.
/// `{"status":"ok","data":...}`
pub fn ok(data: &impl Serialize) {
    let raw = serde_json::to_value(data).unwrap();
    let filtered = apply_filter(&raw);
    let envelope = serde_json::json!({
        "status": "ok",
        "data": filtered,
    });
    println!("{}", serde_json::to_string_pretty(&envelope).unwrap());
}

/// Output a successful action result with a message.
/// `{"status":"ok","message":"..."}`
pub fn ok_msg(message: &str) {
    let envelope = serde_json::json!({
        "status": "ok",
        "message": message,
    });
    println!("{}", serde_json::to_string_pretty(&envelope).unwrap());
}

/// Output an error as JSON to stderr and exit with code 1.
/// `{"status":"error","code":"...","message":"..."}`
pub fn err_json(code: &str, message: &str) -> ! {
    let envelope = serde_json::json!({
        "status": "error",
        "code": code,
        "message": message,
    });
    eprintln!("{}", serde_json::to_string_pretty(&envelope).unwrap());
    std::process::exit(1);
}

pub fn plain(value: &Value) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                match v {
                    Value::Object(_) => {
                        println!("{}:", k);
                        plain_indent(v, 2);
                    }
                    Value::Array(arr) => {
                        println!("{}:", k);
                        for item in arr {
                            plain_indent(item, 2);
                            println!("  ---");
                        }
                    }
                    _ => {
                        println!("{}: {}", k, value_to_string(v));
                    }
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                plain(item);
                println!("---");
            }
        }
        other => println!("{}", value_to_string(other)),
    }
}

fn plain_indent(value: &Value, indent: usize) {
    let pad = " ".repeat(indent);
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                println!("{}{}: {}", pad, k, value_to_string(v));
            }
        }
        other => println!("{}{}", pad, value_to_string(other)),
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "".to_string(),
        other => other.to_string(),
    }
}

pub fn human_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
