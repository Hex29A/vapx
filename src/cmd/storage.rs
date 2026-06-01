use clap::{Args, Subcommand};

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::storage;

#[derive(Args)]
pub struct StorageCmd {
    #[command(subcommand)]
    pub command: StorageCommands,
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
                let (creds, host) = resolve_cam(&cam)?;
                let client = make_client(&host, creds, cam.timeout);
                let resp = storage::list_disks(&client)?;
                let data = resp.get("data").unwrap_or(&resp);
                if cam.plain {
                    format::plain(data);
                } else {
                    format::ok(data);
                }
            }
            StorageCommands::Info { cam, disk } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = make_client(&host, creds, cam.timeout);
                let resp = storage::get_disk_properties(&client, &disk)?;
                let data = resp.get("data").unwrap_or(&resp);
                if cam.plain {
                    format::plain(data);
                } else {
                    format::ok(data);
                }
            }
            StorageCommands::Recordings { cam, max } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = make_client(&host, creds, cam.timeout);
                let data = storage::list_recordings(&client, max)?;
                if cam.plain {
                    format::plain(&data);
                } else {
                    format::ok(&data);
                }
            }
            StorageCommands::Health { cam } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = make_client(&host, creds, cam.timeout);
                let data = storage::get_disk_health(&client)?;
                if cam.plain {
                    format::plain(&data);
                } else {
                    format::ok(&data);
                }
            }
            StorageCommands::Params { cam } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = make_client(&host, creds, cam.timeout);
                let text = storage::get_storage_params(&client)?;
                if cam.plain {
                    println!("{}", text);
                } else {
                    let mut params = serde_json::Map::new();
                    for line in text.lines() {
                        let line = line.trim();
                        if line.is_empty() || line.starts_with('#') {
                            continue;
                        }
                        if let Some((k, v)) = line.split_once('=') {
                            params.insert(k.to_string(), serde_json::json!(v));
                        }
                    }
                    format::ok(&serde_json::Value::Object(params));
                }
            }
        }
        Ok(())
    }
}

fn resolve_cam(cam: &CameraArgs) -> anyhow::Result<(crate::config::credentials::Credentials, String)> {
    resolve(
        &cam.host,
        cam.user.as_deref(),
        cam.pass.as_deref(),
        cam.port,
        cam.insecure,
    )
}

fn make_client(host: &str, creds: crate::config::credentials::Credentials, timeout: Option<u64>) -> VapixClient {
    let t = timeout.unwrap_or(creds.timeout);
    VapixClient::new(host, creds.port, creds, t)
}
