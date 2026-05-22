use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::{Args, Subcommand};

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::params;

#[derive(Args)]
pub struct TemplateCmd {
    #[command(subcommand)]
    pub command: TemplateCommands,
}

#[derive(Subcommand)]
pub enum TemplateCommands {
    /// Create a template from a camera's current parameters
    Create {
        /// Camera to use as template source
        host: String,

        #[arg(short, long, env = "VAPX_USER")]
        user: Option<String>,

        #[arg(short, long, env = "VAPX_PASS")]
        pass: Option<String>,

        #[arg(short = 'k', long)]
        insecure: bool,

        #[arg(long)]
        port: Option<u16>,

        /// Output file (default: template.yaml)
        #[arg(short, long, default_value = "template.yaml")]
        output: PathBuf,

        /// Parameter groups to include (comma-separated, e.g. "root.Image,root.PTZ")
        #[arg(long)]
        groups: Option<String>,

        /// Request timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Apply a template to one or more cameras
    Apply {
        /// Camera IP, hostname, or name from cameras.yaml
        host: String,

        #[arg(short, long, env = "VAPX_USER")]
        user: Option<String>,

        #[arg(short, long, env = "VAPX_PASS")]
        pass: Option<String>,

        #[arg(short = 'k', long)]
        insecure: bool,

        #[arg(long)]
        port: Option<u16>,

        /// Template file to apply
        #[arg(short, long)]
        file: PathBuf,

        /// Preview changes without applying
        #[arg(long)]
        dry_run: bool,

        /// Request timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Show diff between template and camera's current state
    Diff {
        /// Camera IP, hostname, or name from cameras.yaml
        host: String,

        #[arg(short, long, env = "VAPX_USER")]
        user: Option<String>,

        #[arg(short, long, env = "VAPX_PASS")]
        pass: Option<String>,

        #[arg(short = 'k', long)]
        insecure: bool,

        #[arg(long)]
        port: Option<u16>,

        /// Template file to compare against
        #[arg(short, long)]
        file: PathBuf,

        /// Request timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,
    },
}

impl TemplateCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            TemplateCommands::Create { host, user, pass, insecure, port, output, groups, timeout } => {
                let (creds, resolved_host) = resolve(
                    &host, user.as_deref(), pass.as_deref(), port, insecure,
                )?;
                let t = timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds, t);

                let mut all_params = BTreeMap::new();

                if let Some(ref groups_str) = groups {
                    for group in groups_str.split(',') {
                        let group = group.trim();
                        let text = params::list(&client, Some(group))?;
                        for (k, v) in parse_params(&text) {
                            all_params.insert(k, v);
                        }
                    }
                } else {
                    let text = params::list(&client, None)?;
                    all_params = parse_params(&text);
                }

                // Organize into nested YAML structure by group
                let yaml = serde_yaml::to_string(&all_params)?;
                std::fs::write(&output, &yaml)?;

                format::ok(&serde_json::json!({
                    "file": output.display().to_string(),
                    "parameters": all_params.len(),
                    "source": host,
                }));
            }
            TemplateCommands::Apply { host, user, pass, insecure, port, file, dry_run, timeout } => {
                let (creds, resolved_host) = resolve(
                    &host, user.as_deref(), pass.as_deref(), port, insecure,
                )?;
                let t = timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds, t);

                let content = std::fs::read_to_string(&file)?;
                let desired: BTreeMap<String, String> = serde_yaml::from_str(&content)?;

                // Get current state
                let current_text = params::list(&client, None)?;
                let current = parse_params(&current_text);

                // Find differences
                let mut changes: Vec<(String, String, String)> = Vec::new();
                for (k, desired_v) in &desired {
                    match current.get(k) {
                        Some(current_v) if current_v != desired_v => {
                            changes.push((k.clone(), current_v.clone(), desired_v.clone()));
                        }
                        None => {
                            // Parameter doesn't exist on camera, skip
                        }
                        _ => {} // Already matches
                    }
                }

                if changes.is_empty() {
                    format::ok_msg("No changes needed — camera matches template");
                    return Ok(());
                }

                if dry_run {
                    let diff_list: Vec<serde_json::Value> = changes.iter().map(|(k, curr, want)| {
                        serde_json::json!({
                            "param": k,
                            "current": curr,
                            "desired": want,
                        })
                    }).collect();

                    format::ok(&serde_json::json!({
                        "dry_run": true,
                        "changes": diff_list.len(),
                        "diffs": diff_list,
                    }));
                    return Ok(());
                }

                // Apply changes in batches
                let mut ok_count = 0usize;
                let mut err_count = 0usize;
                let mut errors = Vec::new();

                for (k, _curr, want) in &changes {
                    match params::update(&client, &[(k.as_str(), want.as_str())]) {
                        Ok(_) => ok_count += 1,
                        Err(e) => {
                            err_count += 1;
                            errors.push(serde_json::json!({
                                "param": k,
                                "error": format!("{}", e),
                            }));
                        }
                    }
                }

                format::ok(&serde_json::json!({
                    "applied": ok_count,
                    "failed": err_count,
                    "total": changes.len(),
                    "errors": errors,
                }));
            }
            TemplateCommands::Diff { host, user, pass, insecure, port, file, timeout } => {
                let (creds, resolved_host) = resolve(
                    &host, user.as_deref(), pass.as_deref(), port, insecure,
                )?;
                let t = timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds, t);

                let content = std::fs::read_to_string(&file)?;
                let desired: BTreeMap<String, String> = serde_yaml::from_str(&content)?;

                let current_text = params::list(&client, None)?;
                let current = parse_params(&current_text);

                let mut diffs = Vec::new();
                for (k, desired_v) in &desired {
                    match current.get(k) {
                        Some(current_v) if current_v != desired_v => {
                            diffs.push(serde_json::json!({
                                "param": k,
                                "change": "modified",
                                "current": current_v,
                                "desired": desired_v,
                            }));
                        }
                        None => {
                            diffs.push(serde_json::json!({
                                "param": k,
                                "change": "missing_on_camera",
                                "desired": desired_v,
                            }));
                        }
                        _ => {} // matches
                    }
                }

                format::ok(&serde_json::json!({
                    "host": host,
                    "template": file.display().to_string(),
                    "total_diffs": diffs.len(),
                    "diffs": diffs,
                }));
            }
        }
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
