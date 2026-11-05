use clap::{Args, Subcommand, ValueEnum};
use serde_json::json;

use crate::cmd::CameraArgs;
use crate::output::format;
use crate::vapix::o3c::{self, Mode};

#[derive(Args)]
pub struct O3cCmd {
    #[command(subcommand)]
    pub command: O3cCommands,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ModeArg {
    /// Connect to the cloud all the time
    Always,
    /// Connect only after the control button is pressed
    Oneclick,
    /// Never connect (cuts cloud access)
    Off,
}

impl From<ModeArg> for Mode {
    fn from(m: ModeArg) -> Self {
        match m {
            ModeArg::Always => Mode::Always,
            ModeArg::Oneclick => Mode::OneClick,
            ModeArg::Off => Mode::Off,
        }
    }
}

#[derive(Subcommand)]
pub enum O3cCommands {
    /// Mode (always, oneclick, off), whether the camera is assigned to a service, and proxy
    Status {
        #[command(flatten)]
        cam: CameraArgs,
    },
    /// Set the mode. `off` cuts cloud access and needs --yes
    Mode {
        /// always, oneclick or off
        #[arg(value_enum)]
        mode: ModeArg,

        #[command(flatten)]
        cam: CameraArgs,

        /// Allow turning O3C off
        #[arg(short, long)]
        yes: bool,
    },
    /// The Owner Authentication Key and the serial number, for registering the camera with
    /// an O3C service. The key proves ownership: keep it like a password
    Oak {
        #[command(flatten)]
        cam: CameraArgs,

        /// Print only the key
        #[arg(long)]
        only_oak: bool,
    },
}

impl O3cCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            O3cCommands::Status { cam } => {
                let client = cam.client()?;
                format::output(&o3c::status(&client)?, cam.plain);
            }
            O3cCommands::Mode { mode, cam, yes } => {
                let mode = Mode::from(mode);
                let client = cam.client()?;
                if mode == Mode::Off && !yes {
                    let now = o3c::status(&client)?;
                    let out = json!({
                        "performed": false,
                        "mode": now["mode"],
                        "note": "Nothing was changed. Turning O3C off cuts cloud access to the camera; run again with --yes.",
                    });
                    format::output(&out, cam.plain);
                    return Ok(());
                }
                let mut result = o3c::set_mode(&client, mode)?;
                result["performed"] = json!(true);
                format::output(&result, cam.plain);
            }
            O3cCommands::Oak { cam, only_oak } => {
                let client = cam.client()?;
                let key = o3c::oak(&client)?;
                if only_oak {
                    println!("{key}");
                    return Ok(());
                }
                let serial = o3c::serial(&client)?;
                format::output(&json!({"serial": serial, "oak": key}), cam.plain);
            }
        }
        Ok(())
    }
}
