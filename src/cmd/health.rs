use std::sync::Mutex;
use std::time::Instant;

use clap::Args;
use rayon::prelude::*;

use crate::config::cameras;
use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::{device, firmware};

#[derive(Args)]
pub struct HealthCmd {
    /// Camera group or comma-separated camera names/hosts
    pub targets: String,

    /// Output as plain text instead of JSON
    #[arg(long)]
    pub plain: bool,

    /// Request timeout in seconds per camera
    #[arg(long)]
    pub timeout: Option<u64>,
}

impl HealthCmd {
    pub fn run(self) -> anyhow::Result<()> {
        let config = cameras::load_cameras()?
            .ok_or_else(|| anyhow::anyhow!("No cameras.yaml found. Run `vapx config init` to create one."))?;

        let targets = crate::cmd::resolve_targets(&config, &self.targets)?;
        if targets.is_empty() {
            anyhow::bail!("No cameras matched '{}'", self.targets);
        }

        let results: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());
        let timeout = self.timeout;

        targets.par_iter().for_each(|name| {
            let result = check_camera(name, timeout);
            results.lock().unwrap().push(result);
        });

        let results = results.into_inner().unwrap();
        let total = results.len();
        let healthy = results.iter().filter(|r| r["status"].as_str() == Some("healthy")).count();
        let degraded = results.iter().filter(|r| r["status"].as_str() == Some("degraded")).count();
        let unreachable = results.iter().filter(|r| r["status"].as_str() == Some("unreachable")).count();

        if self.plain {
            eprintln!("Fleet Health Report");
            eprintln!("===================");
            for r in &results {
                let name = r["camera"].as_str().unwrap_or("?");
                let status = r["status"].as_str().unwrap_or("?");
                let model = r["model"].as_str().unwrap_or("-");
                let firmware = r["firmware"].as_str().unwrap_or("-");
                let latency = r["latency_ms"].as_u64().map(|l| format!("{}ms", l)).unwrap_or("-".to_string());

                let marker = match status {
                    "healthy" => "✓",
                    "degraded" => "!",
                    "unreachable" => "✗",
                    _ => "?",
                };
                eprintln!(" {} {:<20} {:<12} {:<20} {:<12} {}", marker, name, status, model, firmware, latency);
            }
            eprintln!("---");
            eprintln!("Total: {} | Healthy: {} | Degraded: {} | Unreachable: {}", total, healthy, degraded, unreachable);
        } else {
            format::ok(&serde_json::json!({
                "summary": {
                    "total": total,
                    "healthy": healthy,
                    "degraded": degraded,
                    "unreachable": unreachable,
                },
                "cameras": results,
            }));
        }

        Ok(())
    }
}

fn check_camera(name: &str, timeout: Option<u64>) -> serde_json::Value {
    let start = Instant::now();

    let (creds, resolved_host) = match resolve(name, None, None, None, false) {
        Ok(r) => r,
        Err(e) => {
            return serde_json::json!({
                "camera": name,
                "status": "unreachable",
                "error": format!("{}", e),
            });
        }
    };

    let t = timeout.unwrap_or(creds.timeout.min(10)); // Cap at 10s for health checks
    let client = VapixClient::new(&resolved_host, creds.port, creds, t);

    let mut result = serde_json::json!({
        "camera": name,
        "host": resolved_host,
    });

    let mut issues = Vec::new();

    // Device info
    match device::get_all_properties(&client) {
        Ok(resp) => {
            let data = resp.get("data").unwrap_or(&resp);
            let props = data.get("propertyList").unwrap_or(data);

            if let Some(model) = props.get("Model").and_then(|v| v.as_str()) {
                result["model"] = serde_json::json!(model);
            }
            if let Some(serial) = props.get("SerialNumber").and_then(|v| v.as_str()) {
                result["serial"] = serde_json::json!(serial);
            }
        }
        Err(e) => {
            result["status"] = serde_json::json!("unreachable");
            result["error"] = serde_json::json!(format!("{}", e));
            return result;
        }
    }

    // Firmware status
    match firmware::status(&client) {
        Ok(resp) => {
            let version = resp.pointer("/data/activeFirmwareVersion")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            result["firmware"] = serde_json::json!(version);

            // Check for old firmware
            if let Some(major) = version.split('.').next().and_then(|s| s.parse::<u32>().ok()) {
                if major < 10 {
                    issues.push("Firmware version is very old (pre-10.x)".to_string());
                }
            }
        }
        Err(_) => {
            result["firmware"] = serde_json::json!("unknown");
        }
    }

    let latency = start.elapsed().as_millis() as u64;
    result["latency_ms"] = serde_json::json!(latency);

    if latency > 5000 {
        issues.push(format!("High latency: {}ms", latency));
    }

    if issues.is_empty() {
        result["status"] = serde_json::json!("healthy");
    } else {
        result["status"] = serde_json::json!("degraded");
        result["issues"] = serde_json::json!(issues);
    }

    result
}

