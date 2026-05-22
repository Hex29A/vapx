use std::collections::BTreeMap;

use clap::Args;

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::params;

#[derive(Args)]
pub struct DiffCmd {
    /// First camera IP, hostname, or name from cameras.yaml
    pub host_a: String,

    /// Second camera IP, hostname, or name from cameras.yaml
    pub host_b: String,

    #[arg(short, long, env = "VAPX_USER")]
    pub user: Option<String>,

    #[arg(short, long, env = "VAPX_PASS")]
    pub pass: Option<String>,

    #[arg(short = 'k', long)]
    pub insecure: bool,

    #[arg(long)]
    pub port: Option<u16>,

    /// Parameter group to compare (e.g., "root.Brand")
    #[arg(long)]
    pub group: Option<String>,

    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

impl DiffCmd {
    pub fn run(self) -> anyhow::Result<()> {
        let (creds_a, host_a) = resolve(
            &self.host_a,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        )?;
        let (creds_b, host_b) = resolve(
            &self.host_b,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        )?;

        let timeout_a = self.timeout.unwrap_or(creds_a.timeout);
        let timeout_b = self.timeout.unwrap_or(creds_b.timeout);
        let client_a = VapixClient::new(&host_a, creds_a.port, creds_a, timeout_a);
        let client_b = VapixClient::new(&host_b, creds_b.port, creds_b, timeout_b);

        let text_a = params::list(&client_a, self.group.as_deref())?;
        let text_b = params::list(&client_b, self.group.as_deref())?;

        let map_a = parse_params(&text_a);
        let map_b = parse_params(&text_b);

        let mut diffs = Vec::new();

        // Find changed and removed keys
        for (k, v_a) in &map_a {
            match map_b.get(k) {
                Some(v_b) if v_a != v_b => {
                    diffs.push(serde_json::json!({
                        "param": k,
                        "change": "modified",
                        self.host_a.clone(): v_a,
                        self.host_b.clone(): v_b,
                    }));
                }
                None => {
                    diffs.push(serde_json::json!({
                        "param": k,
                        "change": "only_in_first",
                        self.host_a.clone(): v_a,
                    }));
                }
                _ => {}
            }
        }

        // Find keys only in B
        for (k, v_b) in &map_b {
            if !map_a.contains_key(k) {
                diffs.push(serde_json::json!({
                    "param": k,
                    "change": "only_in_second",
                    self.host_b.clone(): v_b,
                }));
            }
        }

        diffs.sort_by(|a, b| {
            let ka = a["param"].as_str().unwrap_or("");
            let kb = b["param"].as_str().unwrap_or("");
            ka.cmp(kb)
        });

        format::ok(&serde_json::json!({
            "host_a": self.host_a,
            "host_b": self.host_b,
            "total_diffs": diffs.len(),
            "diffs": diffs,
        }));

        Ok(())
    }
}

fn parse_params(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.to_string(), v.to_string());
        }
    }
    map
}
