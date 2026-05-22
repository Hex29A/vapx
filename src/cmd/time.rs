use clap::{Args, Subcommand};

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::time;

#[derive(Args)]
pub struct TimeCmd {
    #[command(subcommand)]
    pub command: TimeCommands,
}

#[derive(Subcommand)]
pub enum TimeCommands {
    /// Show current time/NTP configuration
    Show(TimeShowCmd),
    /// Set time/NTP parameters
    Set(TimeSetCmd),
}

#[derive(Args)]
pub struct TimeShowCmd {
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

#[derive(Args)]
pub struct TimeSetCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// NTP server address
    #[arg(long)]
    pub ntp: Option<String>,
    /// POSIX timezone string (e.g. "CET-1CEST,M3.5.0,M10.5.0/3")
    #[arg(long)]
    pub timezone: Option<String>,
    /// Enable DST
    #[arg(long, conflicts_with = "no_dst")]
    pub dst: bool,
    /// Disable DST
    #[arg(long)]
    pub no_dst: bool,
    /// Sync source: NTP or None
    #[arg(long)]
    pub sync: Option<String>,
    /// Obtain time settings from DHCP (yes/no)
    #[arg(long)]
    pub dhcp: Option<String>,
    /// Raw parameter assignments (root.Time.X=value)
    #[arg(long = "raw", num_args = 1..)]
    pub raw: Vec<String>,
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

impl TimeCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            TimeCommands::Show(cmd) => cmd.run(),
            TimeCommands::Set(cmd) => cmd.run(),
        }
    }
}

impl TimeShowCmd {
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
        let text = time::get_config(&client)?;

        if self.plain {
            print!("{}", text);
        } else {
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
            format::ok(&map);
        }

        Ok(())
    }
}

impl TimeSetCmd {
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

        let mut assignments: Vec<(String, String)> = Vec::new();

        if let Some(ntp) = self.ntp {
            assignments.push(("root.Time.NTP.Server".into(), ntp));
        }
        if let Some(tz) = self.timezone {
            assignments.push(("root.Time.POSIXTimeZone".into(), tz));
        }
        if self.dst {
            assignments.push(("root.Time.DST.Enabled".into(), "yes".into()));
        } else if self.no_dst {
            assignments.push(("root.Time.DST.Enabled".into(), "no".into()));
        }
        if let Some(sync) = self.sync {
            assignments.push(("root.Time.SyncSource".into(), sync));
        }
        if let Some(dhcp) = self.dhcp {
            assignments.push(("root.Time.ObtainFromDHCP".into(), dhcp));
        }

        for raw in &self.raw {
            let (k, v) = raw
                .split_once('=')
                .ok_or_else(|| anyhow::anyhow!("Invalid assignment: {} (expected key=value)", raw))?;
            assignments.push((k.to_string(), v.to_string()));
        }

        if assignments.is_empty() {
            anyhow::bail!("No parameters to set. Use --ntp, --timezone, --dst, --no-dst, --sync, --dhcp, or --raw");
        }

        let kv: Vec<(&str, &str)> = assignments
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let result = time::set_params(&client, &kv)?;
        format::ok_msg(result.trim());
        Ok(())
    }
}
