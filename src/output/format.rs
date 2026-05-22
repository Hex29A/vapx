use std::sync::OnceLock;

use serde::Serialize;
use serde_json::Value;

static FILTER_KEYS: OnceLock<Vec<String>> = OnceLock::new();
static OUTPUT_FORMAT: OnceLock<String> = OnceLock::new();

/// Set the global output filter keys (dot-separated paths like "model" or "firmware.version").
pub fn set_filter(keys: Vec<String>) {
    FILTER_KEYS.set(keys).ok();
}

/// Set the global output format (json, table, csv, yaml).
pub fn set_output_format(fmt: String) {
    OUTPUT_FORMAT.set(fmt).ok();
}

/// Get the current output format (default: "json").
fn output_format() -> &'static str {
    OUTPUT_FORMAT.get().map(|s| s.as_str()).unwrap_or("json")
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

    match output_format() {
        "table" => {
            print_table(&filtered);
        }
        "csv" => {
            print_csv(&filtered);
        }
        "yaml" => {
            let envelope = serde_json::json!({
                "status": "ok",
                "data": filtered,
            });
            println!("{}", serde_yaml::to_string(&envelope).unwrap_or_default());
        }
        _ => {
            let envelope = serde_json::json!({
                "status": "ok",
                "data": filtered,
            });
            println!("{}", serde_json::to_string_pretty(&envelope).unwrap());
        }
    }
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

/// Print data as an ASCII table.
fn print_table(value: &Value) {
    match value {
        Value::Array(arr) if !arr.is_empty() => {
            // Array of objects → tabular
            let headers = collect_keys(arr);
            if headers.is_empty() {
                println!("{}", value);
                return;
            }
            // Compute column widths
            let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
            let rows: Vec<Vec<String>> = arr.iter().map(|item| {
                headers.iter().enumerate().map(|(i, h)| {
                    let s = value_to_string(item.get(h).unwrap_or(&Value::Null));
                    widths[i] = widths[i].max(s.len());
                    s
                }).collect()
            }).collect();

            // Header
            let header_line: Vec<String> = headers.iter().enumerate()
                .map(|(i, h)| format!("{:<width$}", h.to_uppercase(), width = widths[i]))
                .collect();
            println!("{}", header_line.join("  "));
            let sep: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
            println!("{}", sep.join("  "));
            // Rows
            for row in &rows {
                let line: Vec<String> = row.iter().enumerate()
                    .map(|(i, v)| format!("{:<width$}", v, width = widths[i]))
                    .collect();
                println!("{}", line.join("  "));
            }
        }
        Value::Object(map) => {
            // Single object → key-value table
            let max_key = map.keys().map(|k| k.len()).max().unwrap_or(0);
            for (k, v) in map {
                match v {
                    Value::Object(_) | Value::Array(_) => {
                        println!("{:<width$}  {}", k, serde_json::to_string(v).unwrap_or_default(), width = max_key);
                    }
                    _ => {
                        println!("{:<width$}  {}", k, value_to_string(v), width = max_key);
                    }
                }
            }
        }
        _ => println!("{}", value_to_string(value)),
    }
}

/// Print data as CSV.
fn print_csv(value: &Value) {
    match value {
        Value::Array(arr) if !arr.is_empty() => {
            let headers = collect_keys(arr);
            if headers.is_empty() {
                for item in arr {
                    println!("{}", csv_escape(&value_to_string(item)));
                }
                return;
            }
            println!("{}", headers.join(","));
            for item in arr {
                let row: Vec<String> = headers.iter()
                    .map(|h| csv_escape(&value_to_string(item.get(h).unwrap_or(&Value::Null))))
                    .collect();
                println!("{}", row.join(","));
            }
        }
        Value::Object(map) => {
            println!("key,value");
            for (k, v) in map {
                println!("{},{}", csv_escape(k), csv_escape(&value_to_string(v)));
            }
        }
        _ => println!("{}", csv_escape(&value_to_string(value))),
    }
}

/// Collect unique keys from an array of objects, preserving first-seen order.
fn collect_keys(arr: &[Value]) -> Vec<String> {
    let mut keys = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in arr {
        if let Value::Object(map) = item {
            for k in map.keys() {
                if seen.insert(k.clone()) {
                    keys.push(k.clone());
                }
            }
        }
    }
    keys
}

/// Escape a value for CSV (quote if contains comma, quote, or newline).
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
