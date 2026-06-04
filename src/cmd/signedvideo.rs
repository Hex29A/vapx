use clap::{Args, Subcommand};

use crate::cmd::CameraArgs as SignedvideoCameraArgs;
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
    let (creds, resolved_host) = crate::cmd::resolve_cam(
        &args.host,
        args.user.as_deref(),
        args.pass.as_deref(),
        args.port,
        args.insecure,
    )?;
    Ok(crate::cmd::make_client(&resolved_host, creds, args.timeout))
}
