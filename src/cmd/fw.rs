use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::{Args, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};

use crate::config::credentials::{self, resolve};
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::firmware;

#[derive(Args)]
pub struct FwCmd {
    #[command(subcommand)]
    pub command: FwCommands,
}

#[derive(Args, Clone)]
pub struct CameraArgs {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,

    /// Username
    #[arg(short, long, env = "VAPX_USER")]
    pub user: Option<String>,

    /// Password
    #[arg(short, long, env = "VAPX_PASS")]
    pub pass: Option<String>,

    /// Skip TLS certificate verification
    #[arg(short = 'k', long)]
    pub insecure: bool,

    /// Port number
    #[arg(long)]
    pub port: Option<u16>,

    /// Output as plain text instead of JSON
    #[arg(long)]
    pub plain: bool,

    /// Request timeout in seconds (default: 120 for firmware operations)
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[derive(Subcommand)]
pub enum FwCommands {
    /// Show firmware status
    Status {
        #[command(flatten)]
        cam: CameraArgs,
    },
    /// Upload and install firmware (.bin file)
    Upgrade {
        #[command(flatten)]
        cam: CameraArgs,

        /// Path to firmware .bin file
        #[arg(short, long)]
        file: PathBuf,

        /// Factory default mode: none (default), soft, hard
        #[arg(long, default_value = "none")]
        factory_default: String,

        /// Wait for camera to come back online after reboot
        #[arg(long)]
        wait: bool,

        /// Max seconds to wait for reboot (default: 300)
        #[arg(long, default_value_t = 300)]
        wait_timeout: u64,
    },
    /// Commit current firmware (prevents auto-rollback)
    Commit {
        #[command(flatten)]
        cam: CameraArgs,
    },
    /// Rollback to previously installed firmware
    Rollback {
        #[command(flatten)]
        cam: CameraArgs,
    },
    /// Reboot the camera
    Reboot {
        #[command(flatten)]
        cam: CameraArgs,

        /// Wait for camera to come back online
        #[arg(long)]
        wait: bool,

        /// Max seconds to wait for reboot (default: 120)
        #[arg(long, default_value_t = 120)]
        wait_timeout: u64,
    },
    /// Factory default the camera
    FactoryDefault {
        #[command(flatten)]
        cam: CameraArgs,

        /// Mode: soft (keep network settings) or hard (full reset)
        #[arg(long, default_value = "soft")]
        mode: String,
    },
}

impl FwCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            FwCommands::Status { cam } => {
                let (creds, resolved_host) = resolve_cam(&cam)?;
                let client = make_client(&resolved_host, creds, cam.timeout);
                let resp = firmware::status(&client)?;
                let output = resp.get("data").unwrap_or(&resp);
                if cam.plain {
                    format::plain(output);
                } else {
                    format::ok(output);
                }
            }
            FwCommands::Upgrade { cam, file, factory_default, wait, wait_timeout } => {
                if !file.exists() {
                    anyhow::bail!("Firmware file not found: {}", file.display());
                }

                let (creds, resolved_host) = resolve_cam(&cam)?;
                let client = make_client(&resolved_host, creds.clone(), cam.timeout);

                let file_size = std::fs::metadata(&file)?.len();
                eprintln!("Reading firmware: {} ({:.1} MB)", file.display(), file_size as f64 / 1_048_576.0);
                let firmware_data = std::fs::read(&file)?;

                let pre_status = firmware::status(&client)?;
                let old_version = pre_status
                    .pointer("/data/activeFirmwareVersion")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                eprintln!("Current firmware: {}", old_version);

                let fd = if factory_default == "none" { None } else { Some(factory_default.as_str()) };

                eprintln!("Uploading firmware...");
                let upload_start = Instant::now();

                let pb = ProgressBar::new_spinner();
                pb.set_style(ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg}")
                    .unwrap());
                pb.set_message("Uploading firmware...");
                pb.enable_steady_tick(Duration::from_millis(100));

                let resp = firmware::upgrade(&client, &firmware_data, fd, None, None)?;
                pb.finish_and_clear();

                let new_version = resp
                    .pointer("/data/firmwareVersion")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                eprintln!("Upload complete in {:.1}s — new version: {}", upload_start.elapsed().as_secs_f64(), new_version);
                eprintln!("Camera is rebooting...");

                if wait {
                    let pb = ProgressBar::new_spinner();
                    pb.set_style(ProgressStyle::default_spinner()
                        .template("{spinner:.yellow} {msg} [{elapsed_precise}]")
                        .unwrap());
                    pb.set_message("Waiting for reboot...");
                    pb.enable_steady_tick(Duration::from_millis(100));
                    wait_for_reboot(&resolved_host, creds.port, creds, wait_timeout)?;
                    pb.finish_and_clear();
                    eprintln!("Camera is back online.");
                }

                format::ok(&serde_json::json!({
                    "previousVersion": old_version,
                    "newVersion": new_version,
                }));
            }
            FwCommands::Commit { cam } => {
                let (creds, resolved_host) = resolve_cam(&cam)?;
                let client = make_client(&resolved_host, creds, cam.timeout);
                let resp = firmware::commit(&client)?;
                let version = resp
                    .pointer("/data/firmwareVersion")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                format::ok_msg(&format!("Firmware {} committed", version));
            }
            FwCommands::Rollback { cam } => {
                let (creds, resolved_host) = resolve_cam(&cam)?;
                let client = make_client(&resolved_host, creds, cam.timeout);
                eprintln!("Rolling back firmware...");
                firmware::rollback(&client)?;
                format::ok_msg("Rollback initiated — camera is rebooting");
            }
            FwCommands::Reboot { cam, wait, wait_timeout } => {
                let (creds, resolved_host) = resolve_cam(&cam)?;
                let client = make_client(&resolved_host, creds.clone(), cam.timeout);
                eprintln!("Rebooting camera...");
                firmware::reboot(&client)?;

                if wait {
                    wait_for_reboot(&resolved_host, creds.port, creds, wait_timeout)?;
                    eprintln!("Camera is back online.");
                }

                format::ok_msg("Reboot complete");
            }
            FwCommands::FactoryDefault { cam, mode } => {
                let (creds, resolved_host) = resolve_cam(&cam)?;
                let client = make_client(&resolved_host, creds, cam.timeout);
                eprintln!("Factory defaulting camera (mode: {})...", mode);
                firmware::factory_default(&client, &mode)?;
                format::ok_msg(&format!("Factory default ({}) initiated", mode));
            }
        }

        Ok(())
    }
}

fn resolve_cam(cam: &CameraArgs) -> anyhow::Result<(credentials::Credentials, String)> {
    resolve(
        &cam.host,
        cam.user.as_deref(),
        cam.pass.as_deref(),
        cam.port,
        cam.insecure,
    )
}

fn make_client(host: &str, creds: credentials::Credentials, timeout: Option<u64>) -> VapixClient {
    let t = timeout.unwrap_or(120);
    VapixClient::new(host, creds.port, creds, t)
}

/// Wait for camera to come back online after reboot.
fn wait_for_reboot(
    host: &str,
    port: u16,
    creds: credentials::Credentials,
    timeout_secs: u64,
) -> anyhow::Result<()> {
    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);

    // Wait for the camera to go down
    std::thread::sleep(Duration::from_secs(5));

    loop {
        if start.elapsed() > timeout {
            anyhow::bail!("Camera did not come back online within {}s", timeout_secs);
        }

        let probe_client = VapixClient::new(host, port, creds.clone(), 5);
        match firmware::status(&probe_client) {
            Ok(_) => return Ok(()),
            Err(_) => {
                std::thread::sleep(Duration::from_secs(3));
            }
        }
    }
}
