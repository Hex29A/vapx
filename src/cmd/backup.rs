use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::{Args, Subcommand};

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::params;

#[derive(Args)]
pub struct BackupCmd {
    #[command(subcommand)]
    pub command: BackupCommands,
}

#[derive(Subcommand)]
pub enum BackupCommands {
    /// Export parameters to a JSON file
    Save(BackupSaveCmd),
    /// Restore parameters from a JSON file
    Restore(BackupRestoreCmd),
}

#[derive(Args)]
pub struct BackupSaveCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,

    /// Output file (default: <host>-params.json)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    #[arg(short, long, env = "VAPX_USER")]
    pub user: Option<String>,

    #[arg(short, long, env = "VAPX_PASS")]
    pub pass: Option<String>,

    #[arg(short = 'k', long)]
    pub insecure: bool,

    #[arg(long)]
    pub port: Option<u16>,

    /// Parameter group to backup (e.g., "root.Brand")
    #[arg(long)]
    pub group: Option<String>,

    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[derive(Args)]
pub struct BackupRestoreCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,

    /// Input file to restore from
    #[arg(short, long)]
    pub file: PathBuf,

    /// Dry run — show what would be changed without applying
    #[arg(long)]
    pub dry_run: bool,

    #[arg(short, long, env = "VAPX_USER")]
    pub user: Option<String>,

    #[arg(short, long, env = "VAPX_PASS")]
    pub pass: Option<String>,

    #[arg(short = 'k', long)]
    pub insecure: bool,

    #[arg(long)]
    pub port: Option<u16>,

    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

impl BackupCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            BackupCommands::Save(cmd) => cmd.run(),
            BackupCommands::Restore(cmd) => cmd.run(),
        }
    }
}

impl BackupSaveCmd {
    fn run(self) -> anyhow::Result<()> {
        let (creds, resolved_host) = resolve(
            &self.host,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        )?;

        let timeout = self.timeout.unwrap_or(creds.timeout);
        let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
        let text = params::list(&client, self.group.as_deref())?;

        let map = parse_params(&text);

        let output_path = self.output.unwrap_or_else(|| {
            PathBuf::from(format!("{}-params.json", self.host))
        });

        let json = serde_json::to_string_pretty(&map)?;
        std::fs::write(&output_path, &json)?;

        format::ok_msg(&format!(
            "Saved {} parameters to {}",
            map.len(),
            output_path.display()
        ));

        Ok(())
    }
}

impl BackupRestoreCmd {
    fn run(self) -> anyhow::Result<()> {
        if !self.file.exists() {
            anyhow::bail!("Backup file not found: {}", self.file.display());
        }

        let content = std::fs::read_to_string(&self.file)?;
        let backup: BTreeMap<String, String> = serde_json::from_str(&content)?;

        let (creds, resolved_host) = resolve(
            &self.host,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        )?;

        let timeout = self.timeout.unwrap_or(creds.timeout);
        let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);

        // Get current params to find diffs
        let current_text = params::list(&client, None)?;
        let current = parse_params(&current_text);

        let mut changes = Vec::new();
        for (k, v) in &backup {
            let current_val = current.get(k).map(|s| s.as_str());
            if current_val != Some(v.as_str()) {
                changes.push((k.as_str(), v.as_str()));
            }
        }

        if changes.is_empty() {
            format::ok_msg("No changes needed — parameters already match backup");
            return Ok(());
        }

        if self.dry_run {
            let diff: Vec<serde_json::Value> = changes
                .iter()
                .map(|(k, v)| {
                    let cur = current.get(*k).map(|s| s.as_str()).unwrap_or("<missing>");
                    serde_json::json!({
                        "param": k,
                        "current": cur,
                        "backup": v,
                    })
                })
                .collect();

            format::ok(&serde_json::json!({
                "dry_run": true,
                "changes": diff.len(),
                "params": diff,
            }));
            return Ok(());
        }

        let mut ok_count = 0;
        let mut errors = Vec::new();

        for (k, v) in &changes {
            match params::update(&client, &[(k, v)]) {
                Ok(_) => ok_count += 1,
                Err(e) => errors.push(serde_json::json!({
                    "param": k,
                    "error": format!("{:#}", e),
                })),
            }
        }

        format::ok(&serde_json::json!({
            "restored": ok_count,
            "errors": errors.len(),
            "error_details": errors,
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
