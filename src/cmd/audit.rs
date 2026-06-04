use clap::Args;

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::{device, firmware, params};

#[derive(Args)]
pub struct AuditCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,

    #[arg(short, long, env = "VAPX_USER")]
    pub user: Option<String>,

    #[arg(short, long, env = "VAPX_PASS")]
    pub pass: Option<String>,

    #[arg(short = 'k', long)]
    pub insecure: bool,

    #[arg(long)]
    pub port: Option<u16>,

    /// Output as plain text instead of JSON
    #[arg(long)]
    pub plain: bool,

    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

impl AuditCmd {
    pub fn run(self) -> anyhow::Result<()> {
        let (creds, resolved_host) = resolve(
            &self.host,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        )?;
        let timeout = self.timeout.unwrap_or(creds.timeout);
        let client = VapixClient::new(&resolved_host, creds.port, creds.clone(), timeout);

        let findings = collect_findings(&client, &creds);

        // Get device info for summary
        let device_info = device::get_all_properties(&client).ok();
        let model = device_info.as_ref()
            .and_then(|d| d.pointer("/data/propertyList/Model").or(d.get("Model")))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let severity_counts = |level: &str| findings.iter()
            .filter(|f| f["severity"].as_str() == Some(level))
            .count();

        let result = serde_json::json!({
            "host": self.host,
            "model": model,
            "summary": {
                "total": findings.len(),
                "critical": severity_counts("critical"),
                "warning": severity_counts("warning"),
                "info": severity_counts("info"),
            },
            "findings": findings,
        });

        if self.plain {
            print_plain(&self.host, model, &findings);
        } else {
            format::ok(&result);
        }

        Ok(())
    }
}

fn collect_findings(client: &VapixClient, creds: &crate::config::credentials::Credentials) -> Vec<serde_json::Value> {
    let mut findings: Vec<serde_json::Value> = Vec::new();

    // 1. Check if connection is HTTP (not HTTPS)
    if !creds.https {
        findings.push(finding("warning", "UNENCRYPTED_HTTP",
            "Connection uses HTTP (unencrypted). Use HTTPS for production."));
    }

    // 2. Check firmware version
    if let Ok(fw) = firmware::status(client) {
        let version = fw.pointer("/data/activeFirmwareVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        // Check if firmware is very old (major version < 10)
        if let Some(major) = version.split('.').next().and_then(|s| s.parse::<u32>().ok()) {
            if major < 10 {
                findings.push(finding("critical", "OUTDATED_FIRMWARE",
                    &format!("Firmware {} is very old (pre-AXIS OS 10.x). Update recommended.", version)));
            }
        }
    }

    // 3. Check for default credentials
    if creds.user == "root" {
        findings.push(finding("warning", "DEFAULT_USER",
            "Using default 'root' username. Consider creating a named admin user."));
    }

    // 4. Check SSH, UPnP, Bonjour, and other security-relevant params
    if let Ok(text) = params::list(client, Some("root.Network")) {
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                let k = k.trim();
                let v = v.trim();

                if k.ends_with(".SSH.Enabled") && v == "yes" {
                    findings.push(finding("info", "SSH_ENABLED",
                        "SSH is enabled. Disable if not needed for maintenance."));
                }
                if k.ends_with(".UPnP.Enabled") && v == "yes" {
                    findings.push(finding("warning", "UPNP_ENABLED",
                        "UPnP is enabled. This can expose the camera to automatic discovery on untrusted networks."));
                }
                if k.ends_with(".Bonjour.Enabled") && v == "yes" {
                    findings.push(finding("info", "BONJOUR_ENABLED",
                        "Bonjour is enabled. Disable if not needed."));
                }
            }
        }
    }

    // 5. Check if anonymous viewers are enabled
    if let Ok(text) = params::list(client, Some("root.Network.RTSP")) {
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim().contains("AllowAnonymousViewing") && v.trim() == "yes" {
                    findings.push(finding("critical", "ANONYMOUS_VIEWING",
                        "Anonymous RTSP viewing is enabled. Anyone can view the stream without authentication."));
                }
            }
        }
    }

    // 6. Check HTTPS enforcement
    if let Ok(text) = params::list(client, Some("root.System")) {
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim().contains("HTTPSConnection") && v.trim() == "optional" {
                    findings.push(finding("warning", "HTTPS_OPTIONAL",
                        "HTTPS is not enforced. Set to 'required' for production."));
                }
            }
        }
    }

    findings
}

fn print_plain(host: &str, model: &str, findings: &[serde_json::Value]) {
    let severity_counts = |level: &str| findings.iter()
        .filter(|f| f["severity"].as_str() == Some(level))
        .count();

    eprintln!("Security audit: {} ({})", host, model);
    eprintln!("---");
    for f in findings {
        let sev = f["severity"].as_str().unwrap_or("?");
        let code = f["code"].as_str().unwrap_or("?");
        let msg = f["message"].as_str().unwrap_or("?");
        let marker = match sev {
            "critical" => "!!",
            "warning" => "!",
            _ => "-",
        };
        eprintln!(" {} [{}] {}: {}", marker, sev.to_uppercase(), code, msg);
    }
    eprintln!("---");
    eprintln!(
        "Total: {} findings ({} critical, {} warning, {} info)",
        findings.len(),
        severity_counts("critical"),
        severity_counts("warning"),
        severity_counts("info"),
    );
}

fn finding(severity: &str, code: &str, message: &str) -> serde_json::Value {
    serde_json::json!({
        "severity": severity,
        "code": code,
        "message": message,
    })
}
