use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Subcommand, ValueEnum};
use serde_json::{json, Value};

use crate::cmd::CameraArgs;
use crate::output::format;
use crate::vapix::disk::{self, Job, Wait, DEFAULT_DISK};

#[derive(Args)]
pub struct DiskCmd {
    #[command(subcommand)]
    pub command: DiskCommands,
}

/// Where a job is followed from and for how long.
#[derive(Args, Clone)]
pub struct WaitArgs {
    /// Give up following a job after this many seconds (the camera goes on with it)
    #[arg(long, default_value_t = 1800)]
    pub wait_timeout: u64,

    /// Seconds between progress checks
    #[arg(long, default_value_t = 2)]
    pub poll_interval: u64,
}

impl WaitArgs {
    fn wait(&self) -> Wait {
        Wait { interval: Duration::from_secs(self.poll_interval), timeout: Duration::from_secs(self.wait_timeout) }
    }
}

/// Where the passphrase comes from. It is never a command-line value: that would show in
/// the process list and in shell history.
#[derive(Args, Clone)]
pub struct PassphraseArgs {
    /// Environment variable that holds the passphrase
    #[arg(long, default_value = "VAPX_DISK_PASSPHRASE")]
    pub passphrase_env: String,

    /// File that holds the passphrase (must not be readable by others)
    #[arg(long)]
    pub passphrase_file: Option<PathBuf>,
}

impl PassphraseArgs {
    fn read(&self) -> anyhow::Result<disk::Secret> {
        disk::secret_from(self.passphrase_file.as_deref(), &self.passphrase_env, "passphrase")
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Switch {
    On,
    Off,
}

#[derive(Subcommand)]
pub enum DiskCommands {
    /// Encryption, file system, health, usage and number of recordings per disk
    Status {
        #[command(flatten)]
        cam: CameraArgs,

        /// Only this disk (default: all)
        #[arg(long)]
        disk: Option<String>,
    },
    /// Format a disk (erases every recording). Without --yes it only shows what would be erased
    Format {
        #[command(flatten)]
        cam: CameraArgs,

        #[arg(long, default_value = DEFAULT_DISK)]
        disk: String,

        /// File system: ext4, xfs or vfat
        #[arg(long, default_value = "ext4")]
        filesystem: String,

        /// Erase for real. Without it nothing is changed
        #[arg(short, long)]
        yes: bool,

        #[command(flatten)]
        wait: WaitArgs,
    },
    /// Turn disk encryption on or off (erases every recording; the camera formats the card).
    /// Without --yes it only shows what would be erased
    Encrypt {
        /// on or off
        #[arg(value_enum)]
        mode: Switch,

        #[command(flatten)]
        cam: CameraArgs,

        #[arg(long, default_value = DEFAULT_DISK)]
        disk: String,

        #[command(flatten)]
        pass: PassphraseArgs,

        /// Erase for real. Without it nothing is changed
        #[arg(short, long)]
        yes: bool,

        #[command(flatten)]
        wait: WaitArgs,
    },
    /// Change the passphrase of an encrypted disk (the recordings stay). A lost passphrase
    /// makes the card unreadable
    Passphrase {
        #[command(flatten)]
        cam: CameraArgs,

        #[arg(long, default_value = DEFAULT_DISK)]
        disk: String,

        #[command(flatten)]
        pass: PassphraseArgs,

        /// Environment variable that holds the new passphrase
        #[arg(long, default_value = "VAPX_DISK_NEW_PASSPHRASE")]
        new_passphrase_env: String,

        /// File that holds the new passphrase (must not be readable by others)
        #[arg(long)]
        new_passphrase_file: Option<PathBuf>,

        #[command(flatten)]
        wait: WaitArgs,
    },
    /// Mount a disk
    Mount {
        #[command(flatten)]
        cam: CameraArgs,

        #[arg(long, default_value = DEFAULT_DISK)]
        disk: String,

        #[command(flatten)]
        wait: WaitArgs,
    },
    /// Unmount a disk (recording to it stops)
    Unmount {
        #[command(flatten)]
        cam: CameraArgs,

        #[arg(long, default_value = DEFAULT_DISK)]
        disk: String,

        #[command(flatten)]
        wait: WaitArgs,
    },
}

/// Progress goes to stderr so that stdout stays one JSON document.
fn progress(step: &str, job: &Job) {
    match job.progress {
        Some(p) => eprintln!("{step}: {p}%"),
        None => eprintln!("{step}: {}", if job.message.is_empty() { "working" } else { &job.message }),
    }
}

/// The dry run: what would be erased and how to do it.
fn dry_run(action: &str, impact: Value, extra: Value) -> Value {
    let mut out = json!({
        "performed": false,
        "action": action,
        "wouldErase": impact,
        "note": "Nothing was changed. Run again with --yes to erase.",
    });
    if let (Some(o), Some(e)) = (out.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    out
}

impl DiskCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            DiskCommands::Status { cam, disk } => {
                let client = cam.client()?;
                let data = disk::status(&client, disk.as_deref())?;
                format::output(&data, cam.plain);
            }
            DiskCommands::Format { cam, disk: id, filesystem, yes, wait } => {
                let client = cam.client()?;
                let target = disk::find_local(&client, &id)?;
                if !yes {
                    let out = dry_run("format", disk::impact(&client, &target)?, json!({"filesystem": filesystem}));
                    format::output(&out, cam.plain);
                    return Ok(());
                }
                let mut result = disk::format(&client, &id, &filesystem, wait.wait(), progress)?;
                result["performed"] = json!(true);
                format::output(&result, cam.plain);
            }
            DiskCommands::Encrypt { mode, cam, disk: id, pass, yes, wait } => {
                let enable = matches!(mode, Switch::On);
                let client = cam.client()?;
                if !disk::supports_encryption(&client)? {
                    anyhow::bail!("this camera has no disk encryption (Properties.LocalStorage.DiskEncryption is not yes)");
                }
                let target = disk::find_local(&client, &id)?;
                if target.encrypted == enable {
                    anyhow::bail!("disk {} is already {}; nothing to do", id, if enable { "encrypted" } else { "unencrypted" });
                }
                // Read the passphrase before anything is changed, so a missing one stops a batch run early.
                let secret = pass.read();
                if !yes {
                    let out = dry_run(
                        if enable { "encrypt on" } else { "encrypt off" },
                        disk::impact(&client, &target)?,
                        json!({"passphraseAvailable": secret.is_ok()}),
                    );
                    format::output(&out, cam.plain);
                    return Ok(());
                }
                let mut result = disk::set_encryption(&client, &id, enable, &secret?, wait.wait(), progress)?;
                result["performed"] = json!(true);
                format::output(&result, cam.plain);
            }
            DiskCommands::Passphrase { cam, disk: id, pass, new_passphrase_env, new_passphrase_file, wait } => {
                let current = pass.read()?;
                let new = disk::secret_from(new_passphrase_file.as_deref(), &new_passphrase_env, "new passphrase")?;
                let client = cam.client()?;
                let result = disk::change_passphrase(&client, &id, &current, &new, wait.wait())?;
                format::output(&result, cam.plain);
            }
            DiskCommands::Mount { cam, disk: id, wait } => {
                let client = cam.client()?;
                disk::find(&client, &id)?;
                disk::mount(&client, &id, wait.wait(), |j| progress("mount", j))?;
                format::output(&json!({"disk": id, "mounted": true}), cam.plain);
            }
            DiskCommands::Unmount { cam, disk: id, wait } => {
                let client = cam.client()?;
                disk::find(&client, &id)?;
                disk::unmount(&client, &id, wait.wait(), |j| progress("unmount", j))?;
                format::output(&json!({"disk": id, "unmounted": true}), cam.plain);
            }
        }
        Ok(())
    }
}
