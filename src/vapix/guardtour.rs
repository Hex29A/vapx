//! Guard tours, kept in `param.cgi` group `GuardTour`.

use serde_json::{json, Map, Value};

use crate::vapix::client::VapixClient;

pub fn list_tours(client: &VapixClient) -> anyhow::Result<Value> {
    let text = client.get_text("/axis-cgi/param.cgi", &[("action", "list"), ("group", "GuardTour")])?;
    Ok(parse_tours(&text))
}

fn yes(v: &str) -> bool {
    v.eq_ignore_ascii_case("yes")
}

/// Parse `root.GuardTour.G0.Name=...` style lines into `[{id, name, running,
/// camera, random_enabled, time_between, presets: [...]}]`, in first-seen order.
fn parse_tours(text: &str) -> Value {
    if text.trim().is_empty() || text.contains("Error") {
        return json!([]);
    }
    let mut tours: Vec<Map<String, Value>> = Vec::new();
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once('=') else { continue };
        let parts: Vec<&str> = key.split('.').collect();
        if parts.len() < 4 || parts[1] != "GuardTour" {
            continue;
        }
        let id = parts[2];
        let idx = match tours.iter().position(|t| t["id"] == id) {
            Some(i) => i,
            None => {
                let mut t = Map::new();
                t.insert("id".into(), json!(id));
                t.insert("presets".into(), json!([]));
                tours.push(t);
                tours.len() - 1
            }
        };
        let tour = &mut tours[idx];
        let num = |v: &str| v.trim().parse::<i64>().ok().map(Value::from);
        if parts.len() == 4 {
            let (field, val) = match parts[3] {
                "Name" => ("name", Some(json!(value))),
                "Running" => ("running", Some(json!(yes(value)))),
                "CamNbr" => ("camera", num(value)),
                "RandomEnabled" => ("random_enabled", Some(json!(yes(value)))),
                "TimeBetweenSequences" => ("time_between", num(value)),
                _ => continue,
            };
            if let Some(val) = val {
                tour.insert(field.into(), val);
            }
        } else if parts.len() == 6 && parts[3] == "Tour" {
            let preset_id = parts[4];
            let field = match parts[5] {
                "PresetNbr" => "preset_number",
                "MoveSpeed" => "move_speed",
                "WaitTime" => "wait_time",
                _ => continue,
            };
            let presets = tour.get_mut("presets").and_then(Value::as_array_mut).unwrap();
            let pos = match presets.iter().position(|p| p["id"] == preset_id) {
                Some(p) => p,
                None => {
                    presets.push(json!({"id": preset_id}));
                    presets.len() - 1
                }
            };
            if let Some(n) = num(value) {
                presets[pos][field] = n;
            }
        }
    }
    Value::Array(tours.into_iter().map(Value::Object).collect())
}

/// Start or stop a tour by its ID (`G0`, `G1`, ...).
pub fn set_running(client: &VapixClient, tour_id: &str, running: bool) -> anyhow::Result<()> {
    let key = format!("GuardTour.{tour_id}.Running");
    let reply = client.get_text(
        "/axis-cgi/param.cgi",
        &[("action", "update"), (&key, if running { "yes" } else { "no" })],
    )?;
    if reply.contains("Error") {
        anyhow::bail!("param.cgi: {}", reply.trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tours_and_presets() {
        let text = "root.GuardTour.G0.Name=Lobby\nroot.GuardTour.G0.Running=yes\nroot.GuardTour.G0.CamNbr=1\n\
                    root.GuardTour.G0.Tour.T0.PresetNbr=3\nroot.GuardTour.G0.Tour.T0.WaitTime=10\n";
        let v = parse_tours(text);
        assert_eq!(v[0]["id"], "G0");
        assert_eq!(v[0]["name"], "Lobby");
        assert_eq!(v[0]["running"], true);
        assert_eq!(v[0]["camera"], 1);
        assert_eq!(v[0]["presets"][0]["preset_number"], 3);
        assert_eq!(v[0]["presets"][0]["wait_time"], 10);
    }

    #[test]
    fn error_or_empty_gives_no_tours() {
        assert_eq!(parse_tours(""), json!([]));
        assert_eq!(parse_tours("# Error: no such group"), json!([]));
    }
}
