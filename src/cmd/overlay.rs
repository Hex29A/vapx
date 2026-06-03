use clap::{Args, Subcommand};

use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::overlay;

#[derive(Args)]
pub struct OverlayCmd {
    #[command(subcommand)]
    pub command: OverlayCommands,
}

#[derive(Args, Clone)]
pub struct CameraArgs {
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

#[derive(Subcommand)]
pub enum OverlayCommands {
    /// List all overlays
    List {
        #[command(flatten)]
        cam: CameraArgs,
    },
    /// Add a text overlay
    Add {
        #[command(flatten)]
        cam: CameraArgs,

        /// Text to display (supports overlay modifiers like %c for date/time)
        #[arg(short, long)]
        text: String,

        /// Position: topLeft, top, topRight, bottomLeft, bottom, bottomRight
        #[arg(long, default_value = "topLeft")]
        position: String,

        /// Camera/view area number
        #[arg(long, default_value_t = 1)]
        camera: u32,

        /// Font size (0-200)
        #[arg(long)]
        font_size: Option<u32>,

        /// Text color: black, white, red, transparent, semiTransparent
        #[arg(long)]
        color: Option<String>,

        /// Background color: black, white, red, transparent, semiTransparent
        #[arg(long)]
        bg_color: Option<String>,
    },
    /// Update a text overlay
    Set {
        #[command(flatten)]
        cam: CameraArgs,

        /// Overlay identity (from list)
        #[arg(short, long)]
        id: u32,

        /// New text
        #[arg(short, long)]
        text: Option<String>,

        /// New position
        #[arg(long)]
        position: Option<String>,

        /// New font size
        #[arg(long)]
        font_size: Option<u32>,

        /// New text color
        #[arg(long)]
        color: Option<String>,

        /// New background color
        #[arg(long)]
        bg_color: Option<String>,
    },
    /// Remove an overlay
    Remove {
        #[command(flatten)]
        cam: CameraArgs,

        /// Overlay identity to remove
        #[arg(short, long)]
        id: u32,
    },
    /// Show overlay capabilities
    Capabilities {
        #[command(flatten)]
        cam: CameraArgs,
    },
}

impl OverlayCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            OverlayCommands::List { cam } => {
                let client = make_client(&cam)?;
                let resp = overlay::list(&client)?;
                let data = resp.get("data").unwrap_or(&resp);
                if cam.plain {
                    format::plain(data);
                } else {
                    format::ok(data);
                }
            }
            OverlayCommands::Add { cam, text, position, camera, font_size, color, bg_color } => {
                let client = make_client(&cam)?;
                let resp = overlay::add_text(
                    &client,
                    camera,
                    &text,
                    Some(position.as_str()),
                    font_size,
                    color.as_deref(),
                    bg_color.as_deref(),
                )?;
                let data = resp.get("data").unwrap_or(&resp);
                format::ok(data);
            }
            OverlayCommands::Set { cam, id, text, position, font_size, color, bg_color } => {
                let client = make_client(&cam)?;
                overlay::set_text(
                    &client,
                    id,
                    text.as_deref(),
                    position.as_deref(),
                    font_size,
                    color.as_deref(),
                    bg_color.as_deref(),
                )?;
                format::ok_msg(&format!("Overlay {} updated", id));
            }
            OverlayCommands::Remove { cam, id } => {
                let client = make_client(&cam)?;
                overlay::remove(&client, id)?;
                format::ok_msg(&format!("Overlay {} removed", id));
            }
            OverlayCommands::Capabilities { cam } => {
                let client = make_client(&cam)?;
                let resp = overlay::get_capabilities(&client)?;
                let data = resp.get("data").unwrap_or(&resp);
                if cam.plain {
                    format::plain(data);
                } else {
                    format::ok(data);
                }
            }
        }

        Ok(())
    }
}

fn make_client(cam: &CameraArgs) -> anyhow::Result<VapixClient> {
    let (creds, resolved_host) = crate::cmd::resolve_cam(
        &cam.host,
        cam.user.as_deref(),
        cam.pass.as_deref(),
        cam.port,
        cam.insecure,
    )?;
    Ok(crate::cmd::make_client(&resolved_host, creds, cam.timeout))
}
