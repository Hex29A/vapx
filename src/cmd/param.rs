use clap::{Args, Subcommand};

use crate::config::credentials::resolve;
use crate::vapix::client::VapixClient;
use crate::vapix::params;

#[derive(Args)]
pub struct ParamCmd {
    #[command(subcommand)]
    pub command: ParamCommands,
}

#[derive(Subcommand)]
pub enum ParamCommands {
    /// List parameters (optionally filtered by group)
    List(ParamListCmd),
    /// Get a specific parameter value
    Get(ParamGetCmd),
    /// Set parameter values
    Set(ParamSetCmd),
}

#[derive(Args)]
pub struct ParamListCmd {
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
    /// Parameter group to list (e.g., "root.Brand", "Properties.PTZ")
    #[arg(long)]
    pub group: Option<String>,
    /// Output as plain text instead of JSON
    #[arg(long)]
    pub plain: bool,
    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[derive(Args)]
pub struct ParamGetCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// Parameter name (e.g., "root.Brand.Brand")
    pub param: String,
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
pub struct ParamSetCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// Parameter assignments (key=value, e.g., "root.PTZ.Various.V1.ReturnToOverview=0")
    pub assignments: Vec<String>,
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

impl ParamCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            ParamCommands::List(cmd) => cmd.run(),
            ParamCommands::Get(cmd) => cmd.run(),
            ParamCommands::Set(cmd) => cmd.run(),
        }
    }
}

impl ParamListCmd {
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

        if self.plain {
            print!("{}", text);
        } else {
            let map = crate::cmd::param_to_json(&text);
            crate::output::format::ok(&map);
        }

        Ok(())
    }
}

impl ParamGetCmd {
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
        let text = params::list(&client, Some(&self.param))?;

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((_k, v)) = line.split_once('=') {
                println!("{}", v);
            } else {
                println!("{}", line);
            }
        }

        Ok(())
    }
}

impl ParamSetCmd {
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

        let mut kv: Vec<(&str, &str)> = Vec::new();
        for a in &self.assignments {
            let (k, v) = a
                .split_once('=')
                .ok_or_else(|| anyhow::anyhow!("Invalid assignment: {} (expected key=value)", a))?;
            kv.push((k, v));
        }

        let result = params::update(&client, &kv)?;
        crate::output::format::ok_msg(result.trim());
        Ok(())
    }
}
