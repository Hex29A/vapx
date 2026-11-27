use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::{Args, Subcommand, ValueEnum};
use indicatif::{ProgressBar, ProgressStyle};

use crate::config::credentials::{self, resolve};
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::firmware;

use crate::cmd::confirm::confirm;
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

/// How much a factory default wipes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ResetMode {
    /// Keep the network settings
    Soft,
    /// Full reset
    Hard,
}

impl ResetMode {
    fn as_str(self) -> &'static str {
        match self {
            ResetMode::Soft => "soft",
            ResetMode::Hard => "hard",
        }
    }
}

/// The factory default mode of a firmware upgrade: `none` keeps all settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum UpgradeReset {
    /// Keep all settings (default)
    None,
    Soft,
    Hard,
}

impl UpgradeReset {
    fn mode(self) -> Option<ResetMode> {
        match self {
            UpgradeReset::None => None,
            UpgradeReset::Soft => Some(ResetMode::Soft),
            UpgradeReset::Hard => Some(ResetMode::Hard),
        }
    }
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

        /// Factory default after the upgrade
        #[arg(long, value_enum, default_value_t = UpgradeReset::None)]
        factory_default: UpgradeReset,

        /// Skip the confirmation prompt for --factory-default soft|hard (required when stdin is not a terminal)
        #[arg(short, long)]
        yes: bool,

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
        #[arg(long, value_enum, default_value_t = ResetMode::Soft)]
        mode: ResetMode,

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
            FwCommands::Upgrade { cam, file, factory_default, yes, wait, wait_timeout, auto_commit } => {
                run_upgrade(cam, file, factory_default.mode(), yes, wait, wait_timeout, auto_commit)?;
            }
            FwCommands::Commit { cam } => {
                let (creds, resolved_host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&resolved_host, creds, cam.timeout);
                let resp = commit_when_booted(&client, COMMIT_BOOT_WAIT)?;
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
                confirm_factory_default(&cam.host, mode, yes)?;
                let (creds, resolved_host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&resolved_host, creds, cam.timeout);
                firmware::factory_default(&client, mode.as_str())?;
                format::ok_msg(&format!("Factory default ({}) sent, camera is going down", mode.as_str()));
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
    wait_for_reboot(&resolved_host, creds.port, creds, wait_timeout, REBOOT_DOWN_LIMIT)?;
    let secs = started.elapsed().as_secs();
    eprintln!("Camera is back online after {} s", secs);
    format::ok_msg(&format!("Camera back online after {} s", secs));
    Ok(())
}

/// Factory default wipes settings: ask first. Without `--yes` and without a
/// terminal there is nobody to ask, so nothing is done.
fn confirm_factory_default(host: &str, mode: ResetMode, yes: bool) -> anyhow::Result<()> {
    confirm(&format!("Factory default ({}) of {} erases settings on the camera", mode.as_str(), host), yes)
}

#[allow(clippy::too_many_arguments)]
fn run_upgrade(
    cam: CameraArgs,
    file: PathBuf,
    factory_default: Option<ResetMode>,
    yes: bool,
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

    // A factory default after the upgrade erases settings like `fw factory-default`
    // does, so it needs the same confirmation, before anything is sent.
    if let Some(mode) = factory_default {
        confirm_factory_default(&cam.host, mode, yes)?;
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

    let fd = factory_default.map(ResetMode::as_str);

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
    let mut active_version = None;
    let mut verified = None;

    if wait {
        let pb = ProgressBar::new_spinner();
        pb.set_style(ProgressStyle::default_spinner()
            .template("{spinner:.yellow} {msg} [{elapsed_precise}]")
            .unwrap());
        pb.set_message("Waiting for reboot...");
        pb.enable_steady_tick(Duration::from_millis(100));
        let result = finish_upgrade(
            &resolved_host,
            creds.clone(),
            wait_timeout,
            UPGRADE_DOWN_LIMIT,
            &new_version,
            auto_commit,
        );
        pb.finish_and_clear();
        let done = result?;
        eprintln!("Camera is back online, running {}.", done.active_version);
        if done.committed {
            eprintln!("Firmware committed.");
        } else if !done.verified {
            eprintln!(
                "Warning: the camera runs {} but the upload reported {}; the version was not confirmed.",
                done.active_version, new_version
            );
        }
        committed = done.committed;
        verified = Some(done.verified);
        active_version = Some(done.active_version);
    }

    format::ok(&serde_json::json!({
        "previousVersion": old_version,
        "newVersion": new_version,
        "activeVersion": active_version,
        "versionVerified": verified,
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

/// How long a camera may keep answering after a reboot request before the
/// reboot is considered not to have started.
const REBOOT_DOWN_LIMIT: Duration = Duration::from_secs(30);
/// The same after a firmware upload. Flashing takes longer to get going than a
/// plain reboot.
pub(crate) const UPGRADE_DOWN_LIMIT: Duration = Duration::from_secs(90);

/// Wait for camera to come back online after reboot.
///
/// The camera must first be seen going down (within `down_limit`), so a reboot
/// that never started is reported instead of looking like a reboot that
/// finished at once. Polling is quiet: one attempt per probe.
pub(crate) fn wait_for_reboot(
    host: &str,
    port: u16,
    creds: credentials::Credentials,
    timeout_secs: u64,
    down_limit: Duration,
) -> anyhow::Result<()> {
    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    let probe = || firmware::is_up(&VapixClient::new(host, port, creds.clone(), 5));

    let down_limit = down_limit.min(timeout);
    while probe() {
        if start.elapsed() > down_limit {
            anyhow::bail!(
                "Camera still answered {}s after the reboot request; the reboot may not have started",
                down_limit.as_secs()
            );
        }
        std::thread::sleep(Duration::from_millis(500));
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

/// What a waited-for firmware upgrade ended in.
pub(crate) struct UpgradeDone {
    /// The version the camera runs after the reboot.
    pub active_version: String,
    /// The camera runs the version the upload reported.
    pub verified: bool,
    pub committed: bool,
}

/// Wait for the camera to go down and come back after a firmware upload, check
/// that it runs the version the upload reported and, only then and only with
/// `auto_commit`, commit it. A version that does not match, or a camera that
/// never rebooted, is an error and nothing is committed, so the camera's own
/// rollback timer stays armed.
pub(crate) fn finish_upgrade(
    host: &str,
    creds: credentials::Credentials,
    wait_timeout: u64,
    down_limit: Duration,
    expected_version: &str,
    auto_commit: bool,
) -> anyhow::Result<UpgradeDone> {
    let deadline = Instant::now() + Duration::from_secs(wait_timeout);
    wait_for_reboot(host, creds.port, creds.clone(), wait_timeout, down_limit)?;
    let client = VapixClient::new(host, creds.port, creds, 30);
    let active_version = firmware::retry_while_booting(deadline, || {}, || firmware::active_version(&client))?;
    let verified = expected_version != "unknown" && active_version == expected_version;
    if auto_commit && !verified {
        anyhow::bail!(
            "The camera runs {} after the reboot but the upgrade should give {}. Nothing was committed; \
             the rollback timer is still running, and `vapx fw rollback` goes back to the previous firmware.",
            active_version,
            expected_version
        );
    }
    let mut committed = false;
    if auto_commit {
        let result = firmware::retry_while_booting(
            deadline,
            || eprintln!("The camera has not finished booting yet, retrying the commit..."),
            || firmware::commit(&client),
        );
        if let Err(err) = result {
            return Err(uncommitted_error(&client, &active_version, err));
        }
        committed = true;
    }
    Ok(UpgradeDone { active_version, verified, committed })
}

/// How long `vapx fw commit` keeps retrying while the camera says it has not
/// finished booting.
const COMMIT_BOOT_WAIT: Duration = Duration::from_secs(120);

/// Commit the running firmware, retrying for up to `wait` while the camera
/// answers that it has not finished booting.
fn commit_when_booted(client: &VapixClient, wait: Duration) -> anyhow::Result<serde_json::Value> {
    firmware::retry_while_booting(
        Instant::now() + wait,
        || eprintln!("The camera has not finished booting yet, retrying the commit for up to {} s...", wait.as_secs()),
        || firmware::commit(client),
    )
}

/// The error for an auto-commit that did not happen: the firmware runs but is
/// not committed, and the camera rolls back by itself unless someone commits.
fn uncommitted_error(client: &VapixClient, active_version: &str, cause: anyhow::Error) -> anyhow::Error {
    let left = match firmware::time_to_rollback(client) {
        Some(secs) => format!("in about {} s", secs),
        None => "when the commit window runs out (about 10 minutes after the upgrade)".to_string(),
    };
    anyhow::anyhow!(
        "Firmware {} is running but was NOT committed ({:#}). The camera rolls back to the previous \
         firmware {} unless `vapx fw commit` succeeds before then.",
        active_version,
        cause,
        left
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vapix::testserver::{test_creds, Reply, TestServer};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    const STATUS_12_1: &str = r#"{"apiVersion":"1.0","data":{"activeFirmwareVersion":"12.1.0"}}"#;
    const COMMIT_OK: &str = r#"{"apiVersion":"1.0","data":{"firmwareVersion":"12.1.0"}}"#;

    /// A camera that is down for its first status request and then answers
    /// with `STATUS_12_1`. `commits` counts commit requests.
    fn rebooting_camera(commits: Arc<AtomicUsize>) -> TestServer {
        let statuses = AtomicUsize::new(0);
        TestServer::start(move |req| {
            if req.contains("\"method\":\"commit\"") {
                commits.fetch_add(1, Ordering::SeqCst);
                Reply::Json(200, COMMIT_OK)
            } else if statuses.fetch_add(1, Ordering::SeqCst) == 0 {
                Reply::Drop
            } else {
                Reply::Json(200, STATUS_12_1)
            }
        })
    }

    fn finish(server: &TestServer, expected: &str, auto_commit: bool, down: Duration) -> anyhow::Result<UpgradeDone> {
        finish_upgrade("127.0.0.1", test_creds(server.port), 20, down, expected, auto_commit)
    }

    #[test]
    fn upgrade_commits_when_the_camera_runs_the_new_version() {
        let commits = Arc::new(AtomicUsize::new(0));
        let server = rebooting_camera(commits.clone());
        let done = finish(&server, "12.1.0", true, Duration::from_secs(5)).unwrap();
        assert!(done.verified && done.committed);
        assert_eq!(done.active_version, "12.1.0");
        assert_eq!(commits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn upgrade_does_not_commit_a_camera_on_the_wrong_version() {
        let commits = Arc::new(AtomicUsize::new(0));
        let server = rebooting_camera(commits.clone());
        let err = finish(&server, "12.2.0", true, Duration::from_secs(5)).err().expect("must fail");
        let text = err.to_string();
        assert!(text.contains("12.1.0") && text.contains("12.2.0") && text.contains("rollback"), "{text}");
        assert_eq!(commits.load(Ordering::SeqCst), 0, "a mismatch must leave the rollback timer in place");
    }

    #[test]
    fn upgrade_does_not_commit_when_the_new_version_is_unknown() {
        let commits = Arc::new(AtomicUsize::new(0));
        let server = rebooting_camera(commits.clone());
        assert!(finish(&server, "unknown", true, Duration::from_secs(5)).is_err());
        assert_eq!(commits.load(Ordering::SeqCst), 0);
    }

    /// The bug behind #101: a camera that has not gone down yet used to count
    /// as "back online" after a fixed five second sleep.
    #[test]
    fn upgrade_does_not_commit_a_camera_that_never_went_down() {
        let commits = Arc::new(AtomicUsize::new(0));
        let seen = commits.clone();
        let server = TestServer::start(move |req| {
            if req.contains("\"method\":\"commit\"") {
                seen.fetch_add(1, Ordering::SeqCst);
                Reply::Json(200, COMMIT_OK)
            } else {
                Reply::Json(200, STATUS_12_1)
            }
        });
        let err = finish(&server, "12.1.0", true, Duration::from_secs(1)).err().expect("must fail");
        assert!(err.to_string().contains("may not have started"), "{err}");
        assert_eq!(commits.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn upgrade_without_auto_commit_reports_an_unconfirmed_version() {
        let commits = Arc::new(AtomicUsize::new(0));
        let server = rebooting_camera(commits.clone());
        let done = finish(&server, "12.2.0", false, Duration::from_secs(5)).unwrap();
        assert!(!done.verified && !done.committed);
        assert_eq!(done.active_version, "12.1.0");
        assert_eq!(commits.load(Ordering::SeqCst), 0);
    }

    /// The commit answer an M1075-L gave on AXIS OS 12.11 for about a minute
    /// after the upgrade reboot, while systemready already said yes.
    const COMMIT_BOOTING: &str = r#"{"apiVersion":"1.10","method":"commit","error":{"code":500,"message":"The system hasn't finished booting yet."}}"#;
    const STATUS_UNCOMMITTED: &str = r#"{"apiVersion":"1.10","method":"status","data":{"activeFirmwareVersion":"12.1.0","isCommitted":false,"timeToRollback":569,"pendingCommit":"started"}}"#;

    /// A camera that is down for its first status request, then answers
    /// status with `STATUS_UNCOMMITTED` and the first `booting` commits with
    /// `COMMIT_BOOTING`.
    fn booting_camera(booting: usize, commits: Arc<AtomicUsize>) -> TestServer {
        let statuses = AtomicUsize::new(0);
        TestServer::start(move |req| {
            if req.contains("\"method\":\"commit\"") {
                if commits.fetch_add(1, Ordering::SeqCst) < booting {
                    Reply::Json(200, COMMIT_BOOTING)
                } else {
                    Reply::Json(200, COMMIT_OK)
                }
            } else if statuses.fetch_add(1, Ordering::SeqCst) == 0 {
                Reply::Drop
            } else {
                Reply::Json(200, STATUS_UNCOMMITTED)
            }
        })
    }

    /// #110: the commit used to be tried once, so "not finished booting"
    /// ended the upgrade uncommitted and the camera rolled back later.
    #[test]
    fn upgrade_retries_the_commit_while_the_camera_finishes_booting() {
        let commits = Arc::new(AtomicUsize::new(0));
        let server = booting_camera(3, commits.clone());
        let done = finish(&server, "12.1.0", true, Duration::from_secs(5)).unwrap();
        assert!(done.verified && done.committed);
        assert_eq!(commits.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn upgrade_waits_for_a_status_that_says_not_finished_booting() {
        let commits = Arc::new(AtomicUsize::new(0));
        let seen = commits.clone();
        let statuses = AtomicUsize::new(0);
        let server = TestServer::start(move |req| {
            if req.contains("\"method\":\"commit\"") {
                seen.fetch_add(1, Ordering::SeqCst);
                return Reply::Json(200, COMMIT_OK);
            }
            match statuses.fetch_add(1, Ordering::SeqCst) {
                // Down, then up for the reboot probe, then not quite ready.
                0 => Reply::Drop,
                1 => Reply::Json(200, STATUS_12_1),
                2 => Reply::Text(500, "The system hasn't finished booting yet."),
                3 => Reply::Text(503, ""),
                _ => Reply::Json(200, STATUS_12_1),
            }
        });
        let done = finish(&server, "12.1.0", true, Duration::from_secs(5)).unwrap();
        assert!(done.committed);
        assert_eq!(commits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn upgrade_that_cannot_commit_in_time_says_it_will_roll_back() {
        let commits = Arc::new(AtomicUsize::new(0));
        let server = booting_camera(usize::MAX, commits.clone());
        let err = finish_upgrade("127.0.0.1", test_creds(server.port), 2, Duration::from_secs(1), "12.1.0", true)
            .err()
            .expect("must fail");
        let text = err.to_string();
        assert!(text.contains("NOT committed") && text.contains("569 s") && text.contains("vapx fw commit"), "{text}");
        assert!(commits.load(Ordering::SeqCst) > 1, "the commit must be retried");
    }

    #[test]
    fn fw_commit_retries_while_the_camera_finishes_booting() {
        let commits = Arc::new(AtomicUsize::new(0));
        let server = booting_camera(2, commits.clone());
        let resp = commit_when_booted(&server.client(), Duration::from_secs(5)).unwrap();
        assert_eq!(resp.pointer("/data/firmwareVersion").and_then(|v| v.as_str()), Some("12.1.0"));
        assert_eq!(commits.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn other_commit_errors_are_not_retried() {
        let commits = Arc::new(AtomicUsize::new(0));
        let seen = commits.clone();
        let server = TestServer::start(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
            Reply::Json(200, r#"{"apiVersion":"1.10","error":{"code":2,"message":"Nothing to commit"}}"#)
        });
        assert!(commit_when_booted(&server.client(), Duration::from_secs(5)).is_err());
        assert_eq!(commits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn still_booting_is_recognized_in_its_forms() {
        use crate::vapix::client::HttpError;
        let http = |status, message: &str| anyhow::Error::from(HttpError { status, message: message.into() });
        assert!(firmware::is_still_booting(&anyhow::anyhow!("VAPIX error 500: The system hasn't finished booting yet.")));
        assert!(firmware::is_still_booting(&http(500, "The system hasn't finished booting yet.")));
        assert!(firmware::is_still_booting(&http(503, "")));
        assert!(!firmware::is_still_booting(&http(500, "The server encountered an error")));
        assert!(!firmware::is_still_booting(&http(401, "Unauthorized")));
    }
}
