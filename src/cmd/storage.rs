use clap::{Args, Subcommand};

use crate::cmd::CameraArgs;
use crate::output::format;
use crate::vapix::storage;

#[derive(Args)]
pub struct StorageCmd {
    #[command(subcommand)]
    pub command: StorageCommands,
}

#[derive(Subcommand)]
pub enum StorageCommands {
    /// List disks and storage devices
    List {
        #[command(flatten)]
        cam: CameraArgs,
    },
    /// Show disk properties (health, capacity, usage)
    Info {
        #[command(flatten)]
        cam: CameraArgs,

        /// Disk ID (from list output)
        #[arg(long)]
        disk: String,
    },
    /// List recordings on storage
    Recordings {
        #[command(flatten)]
        cam: CameraArgs,

        /// Maximum number of recordings to return
        #[arg(long, default_value = "1000")]
        max: u32,
    },
    /// Show disk health, wear level, and status
    Health {
        #[command(flatten)]
        cam: CameraArgs,
    },
    /// Show storage parameters
    Params {
        #[command(flatten)]
        cam: CameraArgs,
    },
}

impl StorageCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            StorageCommands::List { cam } => {
                let client = cam.client()?;
                let resp = storage::list_disks(&client)?;
                let data = resp.get("data").unwrap_or(&resp);
                format::output(data, cam.plain);
            }
            StorageCommands::Info { cam, disk } => {
                let client = cam.client()?;
                let resp = storage::get_disk_properties(&client, &disk)?;
                let data = resp.get("data").unwrap_or(&resp);
                format::output(data, cam.plain);
            }
            StorageCommands::Recordings { cam, max } => {
                let client = cam.client()?;
                let data = storage::list_recordings(&client, max)?;
                format::output(&data, cam.plain);
            }
            StorageCommands::Health { cam } => {
                let client = cam.client()?;
                let data = storage::get_disk_health(&client)?;
                format::output(&data, cam.plain);
            }
            StorageCommands::Params { cam } => {
                let client = cam.client()?;
                let text = storage::get_storage_params(&client)?;
                if cam.plain {
                    println!("{}", text);
                } else {
                    let params = crate::cmd::param_to_json(&text);
                    format::ok(&serde_json::Value::Object(params));
                }
            }
        }
        Ok(())
    }
}
