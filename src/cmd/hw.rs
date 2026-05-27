use clap::{Args, Subcommand};

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::io;

#[derive(Args)]
pub struct HwCmd {
    #[command(subcommand)]
    pub command: HwCommands,
}

#[derive(Subcommand)]
pub enum HwCommands {
    /// Show I/O port configuration
    Show(HwShowCmd),
    /// Set I/O port parameters
    Set(HwSetCmd),
    /// Trigger output port state (activate/deactivate)
    Trigger(HwTriggerCmd),
}

#[derive(Args)]
pub struct HwShowCmd {
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
pub struct HwSetCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// Port index (e.g. 0, 1, 2, 3)
    #[arg(long)]
    pub index: u8,
    /// Set port direction (input or output)
    #[arg(long)]
    pub direction: Option<String>,
    /// Set port active state (open or closed)
    #[arg(long)]
    pub active: Option<String>,
    /// Set output mode (bistable or pulse)
    #[arg(long)]
    pub mode: Option<String>,
    /// Set pulse time in seconds (for pulse mode)
    #[arg(long)]
    pub pulse_time: Option<u32>,
    /// Raw parameter assignments (root.IOPort.X=value)
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

#[derive(Args)]
pub struct HwTriggerCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// Port index (e.g. 0, 1, 2, 3)
    #[arg(long)]
    pub index: u8,
    /// Output state: active or inactive
    #[arg(long)]
    pub state: String,
    /// Pulse duration in milliseconds (activate, wait, then deactivate)
    #[arg(long)]
    pub pulse: Option<u64>,
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

impl HwCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            HwCommands::Show(cmd) => cmd.run(),
            HwCommands::Set(cmd) => cmd.run(),
            HwCommands::Trigger(cmd) => cmd.run(),
        }
    }
}

impl HwShowCmd {
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
        let text = io::get_ports(&client)?;

        if self.plain {
            print!("{}", text);
        } else {
            // Try parsing as JSON first (from portmanagement API fallback)
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                let data = json.get("data").unwrap_or(&json);
                format::ok(data);
            } else {
                // param.cgi key=value format
                let map = crate::cmd::param_to_json(&text);
                format::ok(&map);
            }
        }

        Ok(())
    }
}

impl HwSetCmd {
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

        let port_id = format!("I{}", self.index);
        let mut assignments: Vec<(String, String)> = Vec::new();

        if let Some(dir) = self.direction {
            assignments.push((format!("root.IOPort.{}.Direction", port_id), dir));
        }
        if let Some(active) = self.active {
            assignments.push((format!("root.IOPort.{}.Output.Active", port_id), active));
        }
        if let Some(mode) = self.mode {
            assignments.push((format!("root.IOPort.{}.Output.Mode", port_id), mode));
        }
        if let Some(pt) = self.pulse_time {
            assignments.push((format!("root.IOPort.{}.Output.PulseTime", port_id), pt.to_string()));
        }

        for raw in &self.raw {
            let (k, v) = raw
                .split_once('=')
                .ok_or_else(|| anyhow::anyhow!("Invalid assignment: {} (expected key=value)", raw))?;
            assignments.push((k.to_string(), v.to_string()));
        }

        if assignments.is_empty() {
            anyhow::bail!("No parameters to set. Use --direction, --active, --mode, --pulse-time, or --raw");
        }

        let kv: Vec<(&str, &str)> = assignments
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let result = io::set_params(&client, &kv)?;
        format::ok_msg(result.trim());
        Ok(())
    }
}

impl HwTriggerCmd {
    fn run(self) -> anyhow::Result<()> {
        let active = match self.state.to_lowercase().as_str() {
            "active" | "on" | "1" | "close" | "closed" => true,
            "inactive" | "off" | "0" | "open" => false,
            _ => anyhow::bail!("Invalid state '{}'. Use: active, inactive, on, off", self.state),
        };

        let (creds, resolved_host) = resolve(
            &self.host,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        )?;
        let timeout = self.timeout.unwrap_or(creds.timeout);
        let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);

        io::trigger_port(&client, self.index, active)?;

        if let Some(pulse_ms) = self.pulse {
            format::ok_msg(&format!("Port {} activated, pulsing for {}ms", self.index, pulse_ms));
            std::thread::sleep(std::time::Duration::from_millis(pulse_ms));
            io::trigger_port(&client, self.index, !active)?;
            format::ok_msg(&format!("Port {} deactivated after {}ms pulse", self.index, pulse_ms));
        } else {
            let state_str = if active { "active" } else { "inactive" };
            format::ok_msg(&format!("Port {} set to {}", self.index, state_str));
        }

        Ok(())
    }
}
