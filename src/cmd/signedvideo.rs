use clap::{Args, Subcommand};

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::signedvideo;

#[derive(Args)]
pub struct SignedvideoCmd {
    #[command(subcommand)]
    pub command: SignedvideoCommands,
}

#[derive(Subcommand)]
pub enum SignedvideoCommands {
    /// Show signed video status
    Status(SignedvideoCameraArgs),
    /// Enable signed video
    Enable(SignedvideoCameraArgs),
    /// Disable signed video
    Disable(SignedvideoCameraArgs),
}

#[derive(Args)]
pub struct SignedvideoCameraArgs {
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

impl SignedvideoCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            SignedvideoCommands::Status(args) => {
                let client = make_client(&args)?;
                let result = signedvideo::get_status(&client)?;
                format::ok(&result.get("data").unwrap_or(&result));
                Ok(())
            }
            SignedvideoCommands::Enable(args) => {
                let client = make_client(&args)?;
                signedvideo::enable(&client)?;
                format::ok_msg("Signed video enabled");
                Ok(())
            }
            SignedvideoCommands::Disable(args) => {
                let client = make_client(&args)?;
                signedvideo::disable(&client)?;
                format::ok_msg("Signed video disabled");
                Ok(())
            }
        }
    }
}

fn make_client(args: &SignedvideoCameraArgs) -> anyhow::Result<VapixClient> {
    let (creds, resolved_host) = resolve(
        &args.host,
        args.user.as_deref(),
        args.pass.as_deref(),
        args.port,
        args.insecure,
    )?;
    let timeout = args.timeout.unwrap_or(creds.timeout);
    Ok(VapixClient::new(&resolved_host, creds.port, creds, timeout))
}
