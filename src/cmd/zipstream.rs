use clap::{Args, Subcommand};

use crate::cmd::CameraArgs as ZipstreamCameraArgs;
use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::zipstream;

#[derive(Args)]
pub struct ZipstreamCmd {
    #[command(subcommand)]
    pub command: ZipstreamCommands,
}

#[derive(Subcommand)]
pub enum ZipstreamCommands {
    /// Show ZipStream profiles and settings
    Status(ZipstreamCameraArgs),
    /// Set ZipStream profile level
    Set(ZipstreamSetCmd),
}

#[derive(Args)]
pub struct ZipstreamSetCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// ZipStream profile name (classic, storage, networkloadbalancing)
    #[arg(long)]
    pub profile: String,
    /// ZipStream strength level (0-100)
    #[arg(long)]
    pub level: u32,
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

impl ZipstreamCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            ZipstreamCommands::Status(args) => {
                let (creds, resolved_host) = resolve(
                    &args.host,
                    args.user.as_deref(),
                    args.pass.as_deref(),
                    args.port,
                    args.insecure,
                )?;
                let timeout = args.timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
                let xml = zipstream::list_profiles(&client)?;

                if args.plain {
                    println!("{}", xml);
                } else {
                    let profiles = zipstream::parse_profiles(&xml)?;
                    format::ok(&profiles);
                }
                Ok(())
            }
            ZipstreamCommands::Set(cmd) => {
                let (creds, resolved_host) = resolve(
                    &cmd.host,
                    cmd.user.as_deref(),
                    cmd.pass.as_deref(),
                    cmd.port,
                    cmd.insecure,
                )?;
                let timeout = cmd.timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
                let _resp = zipstream::set_profile(&client, &cmd.profile, cmd.level)?;
                format::ok_msg(&format!(
                    "ZipStream profile '{}' set to level {}",
                    cmd.profile, cmd.level
                ));
                Ok(())
            }
        }
    }
}
