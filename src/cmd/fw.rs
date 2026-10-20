use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::{Args, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};

use crate::config::credentials::{self, resolve};
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::firmware;

use crate::cmd::CameraArgs;

#[derive(Args)]
pub struct FwCmd {
    #[command(subcommand)]
    pub command: FwCommands,
}

/// Arguments for `vapx reboot` and `vapx fw reboot`.
#[derive(Args)]
pub struct RebootArgs {
    #[command(flatten)]
    pub cam: CameraArgs,

    /// Wait for camera to come back online
    #[arg(long)]
    pub wait: bool,

    /// Max seconds to wait for reboot (default: 120)
    #[arg(long, default_value_t = 120)]
    pub wait_timeout: u64,
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

        /// Automatically commit firmware after successful reboot (requires --wait)
        #[arg(long)]
        auto_commit: bool,
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
    Reboot(RebootArgs),
    /// Factory default the camera (asks for confirmation)
    FactoryDefault {
        #[command(flatten)]
        cam: CameraArgs,

        /// Mode: soft (keep network settings) or hard (full reset)
        #[arg(long, default_value = "soft")]
        mode: String,

        /// Skip the confirmation prompt (required when stdin is not a terminal)
        #[arg(short, long)]
        yes: bool,
    },
    /// Check firmware version against another camera or expected version
    Check {
        #[command(flatten)]
        cam: CameraArgs,

        /// Expected firmware version (e.g., "12.1.0")
        #[arg(long)]
        expected: Option<String>,

        /// Compare against another camera (also accepts second positional arg)
        #[arg(long)]
        compare: Option<String>,

        /// Second camera to compare firmware with (alternative to --compare)
        #[arg(value_name = "CAMERA_B")]
        other: Option<String>,
    },
}

impl FwCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            FwCommands::Status { cam } => {
                let (creds, resolved_host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&resolved_host, creds, cam.timeout);
                let resp = firmware::status(&client)?;
                let output = resp.get("data").unwrap_or(&resp);
                format::output(output, cam.plain);
            }
            FwCommands::Upgrade { cam, file, factory_default, wait, wait_timeout, auto_commit } => {
                run_upgrade(cam, file, factory_default, wait, wait_timeout, auto_commit)?;
            }
            FwCommands::Commit { cam } => {
                let (creds, resolved_host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&resolved_host, creds, cam.timeout);
                let resp = firmware::commit(&client)?;
                let version = resp
                    .pointer("/data/firmwareVersion")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                format::ok_msg(&format!("Firmware {} committed", version));
            }
            FwCommands::Rollback { cam } => {
                let (creds, resolved_host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&resolved_host, creds, cam.timeout);
                eprintln!("Rolling back firmware...");
                firmware::rollback(&client)?;
                format::ok_msg("Rollback initiated — camera is rebooting");
            }
            FwCommands::Reboot(args) => run_reboot(args)?,
            FwCommands::FactoryDefault { cam, mode, yes } => {
                confirm_factory_default(&cam.host, &mode, yes)?;
                let (creds, resolved_host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&resolved_host, creds, cam.timeout);
                firmware::factory_default(&client, &mode)?;
                format::ok_msg(&format!("Factory default ({}) sent, camera is going down", mode));
            }
            FwCommands::Check { cam, expected, compare, other } => {
                run_check(cam, expected, compare, other)?;
            }
        }

        Ok(())
    }
}

/// Reboot the camera. Also behind the top-level `vapx reboot`.
pub fn run_reboot(args: RebootArgs) -> anyhow::Result<()> {
    let RebootArgs { cam, wait, wait_timeout } = args;
    let (creds, resolved_host) = resolve_cam(&cam)?;
    let client = crate::cmd::make_client(&resolved_host, creds.clone(), cam.timeout);
    let started = Instant::now();

    // Sent once. The camera usually closes the connection while going down,
    // which firmware::reboot treats as accepted.
    firmware::reboot(&client)?;
    eprintln!("Reboot sent, camera is going down");

    if !wait {
        format::ok_msg("Reboot sent, camera is going down");
        return Ok(());
    }

    eprintln!("Waiting for the camera to come back (up to {}s)...", wait_timeout);
    wait_for_reboot(&resolved_host, creds.port, creds, wait_timeout, true)?;
    let secs = started.elapsed().as_secs();
    eprintln!("Camera is back online after {} s", secs);
    format::ok_msg(&format!("Camera back online after {} s", secs));
    Ok(())
}

/// Factory default wipes settings: ask first. Without `--yes` and without a
/// terminal there is nobody to ask, so nothing is done.
fn confirm_factory_default(host: &str, mode: &str, yes: bool) -> anyhow::Result<()> {
    if yes {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "Factory default ({}) of {} erases settings on the camera. Nothing was done; pass --yes to confirm.",
            mode,
            host
        );
    }
    eprint!("Factory default ({}) of {} erases settings. Type 'yes' to continue: ", mode, host);
    std::io::stderr().flush().ok();
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    if answer.trim() != "yes" {
        anyhow::bail!("Aborted, nothing was done.");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_upgrade(
    cam: CameraArgs,
    file: PathBuf,
    factory_default: String,
    wait: bool,
    wait_timeout: u64,
    auto_commit: bool,
) -> anyhow::Result<()> {
    if auto_commit && !wait {
        anyhow::bail!("--auto-commit requires --wait");
    }

    if !file.exists() {
        anyhow::bail!("Firmware file not found: {}", file.display());
    }

    let (creds, resolved_host) = resolve_cam(&cam)?;
    // Firmware timeout priority: CLI --timeout > fw_timeout from config > timeout from config > 300s
    let fw_timeout = cam.timeout.or_else(|| resolve_fw_timeout(&cam.host));
    let client = crate::cmd::make_client(&resolved_host, creds.clone(),
        Some(fw_timeout.unwrap_or(300)));

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

    let pb = ProgressBar::new(firmware_data.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{bar:30.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec})")
        .unwrap()
        .progress_chars("█▉▊▋▌▍▎▏ "));

    let resp = firmware::upgrade_with_progress(&client, &firmware_data, fd, None, None, &pb)?;
    pb.finish_and_clear();

    let new_version = resp
        .pointer("/data/firmwareVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    eprintln!("Upload complete in {:.1}s — new version: {}", upload_start.elapsed().as_secs_f64(), new_version);
    eprintln!("Camera is rebooting...");

    let mut committed = false;

    if wait {
        let pb = ProgressBar::new_spinner();
        pb.set_style(ProgressStyle::default_spinner()
            .template("{spinner:.yellow} {msg} [{elapsed_precise}]")
            .unwrap());
        pb.set_message("Waiting for reboot...");
        pb.enable_steady_tick(Duration::from_millis(100));
        wait_for_reboot(&resolved_host, creds.port, creds.clone(), wait_timeout, false)?;
        pb.finish_and_clear();
        eprintln!("Camera is back online.");

        if auto_commit {
            let commit_client = crate::cmd::make_client(&resolved_host, creds, cam.timeout);
            firmware::commit(&commit_client)?;
            eprintln!("Firmware committed.");
            committed = true;
        }
    }

    format::ok(&serde_json::json!({
        "previousVersion": old_version,
        "newVersion": new_version,
        "committed": committed,
    }));
    Ok(())
}

fn run_check(
    cam: CameraArgs,
    expected: Option<String>,
    compare: Option<String>,
    other: Option<String>,
) -> anyhow::Result<()> {
    let (creds, resolved_host) = resolve_cam(&cam)?;
    let client = crate::cmd::make_client(&resolved_host, creds, cam.timeout);
    let resp = firmware::status(&client)?;
    let current = resp
        .pointer("/data/activeFirmwareVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // --compare flag takes precedence, positional arg is fallback
    let compare_target = compare.or(other);

    if let Some(ref expected_ver) = expected {
        let matches = current == *expected_ver;
        let result = serde_json::json!({
            "host": cam.host,
            "current": current,
            "expected": expected_ver,
            "matches": matches,
        });
        if cam.plain {
            if matches {
                eprintln!("OK: {} is on expected firmware {}", cam.host, current);
            } else {
                eprintln!("MISMATCH: {} has {} (expected {})", cam.host, current, expected_ver);
            }
        } else {
            format::ok(&result);
        }
    } else if let Some(ref other) = compare_target {
        let (creds_b, host_b) = resolve(
            other,
            cam.user.as_deref(),
            cam.pass.as_deref(),
            cam.port,
            cam.insecure,
        )?;
        let client_b = crate::cmd::make_client(&host_b, creds_b, cam.timeout);
        let resp_b = firmware::status(&client_b)?;
        let other_ver = resp_b
            .pointer("/data/activeFirmwareVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let matches = current == other_ver;
        let result = serde_json::json!({
            "host_a": cam.host,
            "version_a": current,
            "host_b": other,
            "version_b": other_ver,
            "matches": matches,
        });
        if cam.plain {
            if matches {
                eprintln!("OK: Both on firmware {}", current);
            } else {
                eprintln!("{}: {}  |  {}: {}", cam.host, current, other, other_ver);
            }
        } else {
            format::ok(&result);
        }
    } else {
        // Just show current version
        format::ok(&serde_json::json!({
            "host": cam.host,
            "firmware": current,
        }));
    }
    Ok(())
}

fn resolve_cam(cam: &CameraArgs) -> anyhow::Result<(credentials::Credentials, String)> {
    crate::cmd::resolve_cam(
        &cam.host,
        cam.user.as_deref(),
        cam.pass.as_deref(),
        cam.port,
        cam.insecure,
    )
}

/// Look up `fw_timeout` from cameras.yaml for this camera.
fn resolve_fw_timeout(host: &str) -> Option<u64> {
    let config = crate::config::cameras::load_cameras().ok()??;
    let (_, entry) = config.find(host)?;
    entry.fw_timeout
}

/// Wait for camera to come back online after reboot.
///
/// With `confirm_down` the camera must first be seen going down (within 30 s),
/// so a reboot that never started is reported instead of looking like a
/// reboot that finished at once. Polling is quiet: one attempt per probe.
fn wait_for_reboot(
    host: &str,
    port: u16,
    creds: credentials::Credentials,
    timeout_secs: u64,
    confirm_down: bool,
) -> anyhow::Result<()> {
    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    let probe = || firmware::is_up(&VapixClient::new(host, port, creds.clone(), 5));

    if confirm_down {
        let down_limit = Duration::from_secs(30).min(timeout);
        while probe() {
            if start.elapsed() > down_limit {
                anyhow::bail!(
                    "Camera still answered {}s after the reboot request; the reboot may not have started",
                    down_limit.as_secs()
                );
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    } else {
        // Wait for the camera to go down
        std::thread::sleep(Duration::from_secs(5));
    }

    loop {
        if start.elapsed() > timeout {
            anyhow::bail!("Camera did not come back online within {}s", timeout_secs);
        }
        if probe() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}
