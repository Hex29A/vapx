use clap::{Args, Subcommand};

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::viewarea;

#[derive(Args)]
pub struct ViewareaCmd {
    #[command(subcommand)]
    pub command: ViewareaCommands,
}

#[derive(Subcommand)]
pub enum ViewareaCommands {
    /// List all view areas
    List(ViewareaCameraArgs),
    /// Get details for a specific view area
    Get(ViewareaGetCmd),
    /// Set view area geometry (position and size)
    Set(ViewareaSetCmd),
}

#[derive(Args)]
pub struct ViewareaCameraArgs {
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
    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[derive(Args)]
pub struct ViewareaGetCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// View area ID
    #[arg(long)]
    pub id: i64,
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
pub struct ViewareaSetCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// View area ID
    #[arg(long)]
    pub id: i64,
    /// Horizontal offset
    #[arg(long)]
    pub x: i32,
    /// Vertical offset
    #[arg(long)]
    pub y: i32,
    /// Horizontal size (width)
    #[arg(long)]
    pub width: i32,
    /// Vertical size (height)
    #[arg(long)]
    pub height: i32,
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

impl ViewareaCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            ViewareaCommands::List(args) => {
                let (creds, resolved_host) = resolve(
                    &args.host,
                    args.user.as_deref(),
                    args.pass.as_deref(),
                    args.port,
                    args.insecure,
                )?;
                let timeout = args.timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
                let result = viewarea::list(&client)?;
                format::ok(&result.get("data").unwrap_or(&result));
                Ok(())
            }
            ViewareaCommands::Get(cmd) => {
                let (creds, resolved_host) = resolve(
                    &cmd.host,
                    cmd.user.as_deref(),
                    cmd.pass.as_deref(),
                    cmd.port,
                    cmd.insecure,
                )?;
                let timeout = cmd.timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
                let result = viewarea::get_info(&client, cmd.id)?;
                format::ok(&result.get("data").unwrap_or(&result));
                Ok(())
            }
            ViewareaCommands::Set(cmd) => {
                let (creds, resolved_host) = resolve(
                    &cmd.host,
                    cmd.user.as_deref(),
                    cmd.pass.as_deref(),
                    cmd.port,
                    cmd.insecure,
                )?;
                let timeout = cmd.timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
                viewarea::set_geometry(&client, cmd.id, cmd.x, cmd.y, cmd.width, cmd.height)?;
                format::ok_msg(&format!("View area {} geometry updated", cmd.id));
                Ok(())
            }
        }
    }
}
