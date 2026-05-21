use clap::{Args, Subcommand};

use crate::config::credentials::resolve;
use crate::vapix::client::VapixClient;
use crate::vapix::network;

#[derive(Args)]
pub struct NetCmd {
    #[command(subcommand)]
    pub command: NetCommands,
}

#[derive(Subcommand)]
pub enum NetCommands {
    /// Show current network configuration
    Show(NetShowCmd),
    /// Set network parameters
    Set(NetSetCmd),
}

#[derive(Args)]
pub struct NetShowCmd {
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
pub struct NetSetCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// IP address
    #[arg(long)]
    pub ip: Option<String>,
    /// Subnet mask
    #[arg(long)]
    pub mask: Option<String>,
    /// Default gateway
    #[arg(long)]
    pub gateway: Option<String>,
    /// Primary DNS server
    #[arg(long)]
    pub dns1: Option<String>,
    /// Secondary DNS server
    #[arg(long)]
    pub dns2: Option<String>,
    /// Hostname
    #[arg(long)]
    pub hostname: Option<String>,
    /// Enable DHCP
    #[arg(long, conflicts_with = "static_ip")]
    pub dhcp: bool,
    /// Use static IP (disable DHCP)
    #[arg(long, name = "static_ip")]
    pub static_ip: bool,
    /// Raw parameter assignments (root.Network.X=value)
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

impl NetCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            NetCommands::Show(cmd) => cmd.run(),
            NetCommands::Set(cmd) => cmd.run(),
        }
    }
}

impl NetShowCmd {
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
        let text = network::get_config(&client)?;

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
            crate::output::format::json(&map);
        }

        Ok(())
    }
}

impl NetSetCmd {
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

        if self.dhcp {
            assignments.push(("root.Network.BootProto".into(), "dhcp".into()));
        } else if self.static_ip {
            assignments.push(("root.Network.BootProto".into(), "none".into()));
        }
        if let Some(ip) = self.ip {
            assignments.push(("root.Network.IPAddress".into(), ip));
        }
        if let Some(mask) = self.mask {
            assignments.push(("root.Network.SubnetMask".into(), mask));
        }
        if let Some(gw) = self.gateway {
            assignments.push(("root.Network.DefaultRouter".into(), gw));
        }
        if let Some(dns1) = self.dns1 {
            assignments.push(("root.Network.Resolver.NameServer1".into(), dns1));
        }
        if let Some(dns2) = self.dns2 {
            assignments.push(("root.Network.Resolver.NameServer2".into(), dns2));
        }
        if let Some(hostname) = self.hostname {
            assignments.push(("root.Network.HostName".into(), hostname));
        }

        for raw in &self.raw {
            let (k, v) = raw
                .split_once('=')
                .ok_or_else(|| anyhow::anyhow!("Invalid assignment: {} (expected key=value)", raw))?;
            assignments.push((k.to_string(), v.to_string()));
        }

        if assignments.is_empty() {
            anyhow::bail!("No parameters to set. Use --ip, --mask, --gateway, --dns1, --dns2, --hostname, --dhcp, --static-ip, or --raw");
        }

        let kv: Vec<(&str, &str)> = assignments
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let result = network::set_params(&client, &kv)?;
        eprintln!("{}", result.trim());
        Ok(())
    }
}
