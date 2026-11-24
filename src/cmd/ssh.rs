use clap::{Args, Subcommand};

use crate::cmd::confirm::confirm;
use crate::cmd::pwd::PwdArgs;
use crate::cmd::CameraArgs;
use crate::output::format;
use crate::vapix::ssh;

#[derive(Args)]
pub struct SshCmd {
    #[command(subcommand)]
    pub command: SshCommands,
}

#[derive(Subcommand)]
pub enum SshCommands {
    /// Show whether the SSH service is enabled
    Status {
        #[command(flatten)]
        cam: CameraArgs,
    },
    /// Turn the SSH service on
    Enable {
        #[command(flatten)]
        cam: CameraArgs,
    },
    /// Turn the SSH service off
    Disable {
        #[command(flatten)]
        cam: CameraArgs,
    },
    /// Manage SSH accounts (needs AXIS OS 11.5 or later; see `ssh status`)
    Users(SshUsersCmd),
}

#[derive(Args)]
pub struct SshUsersCmd {
    #[command(subcommand)]
    pub command: SshUsersCommands,
}

#[derive(Subcommand)]
pub enum SshUsersCommands {
    /// List SSH accounts
    List {
        #[command(flatten)]
        cam: CameraArgs,
    },
    /// Create an SSH account
    Add {
        #[command(flatten)]
        cam: CameraArgs,

        /// Account name (1-32 of a-z A-Z 0-9 _ - .)
        #[arg(long)]
        name: String,

        #[command(flatten)]
        pwd: PwdArgs,

        /// Account description
        #[arg(long, default_value = "")]
        comment: String,
    },
    /// Change the password of an SSH account
    Passwd {
        #[command(flatten)]
        cam: CameraArgs,

        /// Account name
        #[arg(long)]
        name: String,

        #[command(flatten)]
        pwd: PwdArgs,
    },
    /// Remove an SSH account
    Remove {
        #[command(flatten)]
        cam: CameraArgs,

        /// Account name to remove
        #[arg(long)]
        name: String,

        /// Skip the confirmation prompt (required when stdin is not a terminal)
        #[arg(short, long)]
        yes: bool,

        /// Show whether the account exists, without removing anything
        #[arg(long)]
        dry_run: bool,
    },
}

/// The account names in the `data` array of the account list, as plain strings or as objects.
fn account_names(accounts: &[serde_json::Value]) -> Vec<&str> {
    accounts
        .iter()
        .filter_map(|u| u.as_str().or_else(|| u.get("username").or_else(|| u.get("name")).and_then(|n| n.as_str())))
        .collect()
}

impl SshCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            SshCommands::Status { cam } => {
                let client = cam.client()?;
                let enabled = ssh::is_enabled(&client)?;
                let api = ssh::api_versions(&client);
                let firmware = ssh::firmware_version(&client);
                let login = firmware.as_deref().and_then(ssh::login_mode);
                format::output(
                    &serde_json::json!({
                        "enabled": enabled,
                        "account_api": api,
                        "firmware": firmware,
                        "login": login,
                    }),
                    cam.plain,
                );
            }
            SshCommands::Enable { cam } => {
                ssh::set_enabled(&cam.client()?, true)?;
                format::ok_msg("SSH enabled");
            }
            SshCommands::Disable { cam } => {
                ssh::set_enabled(&cam.client()?, false)?;
                format::ok_msg("SSH disabled");
            }
            SshCommands::Users(cmd) => cmd.run()?,
        }
        Ok(())
    }
}

impl SshUsersCmd {
    fn run(self) -> anyhow::Result<()> {
        match self.command {
            SshUsersCommands::List { cam } => {
                let users = ssh::list_users(&cam.client()?)?;
                format::output(&users, cam.plain);
            }
            SshUsersCommands::Add { cam, name, pwd, comment } => {
                let pwd = pwd.read(&format!("the new SSH account {name}"), true)?;
                ssh::add_user(&cam.client()?, &name, &pwd, &comment)?;
                format::ok_msg(&format!("SSH user {} created", name));
            }
            SshUsersCommands::Passwd { cam, name, pwd } => {
                let pwd = pwd.read(&format!("SSH account {name}"), true)?;
                ssh::set_password(&cam.client()?, &name, &pwd)?;
                format::ok_msg(&format!("Password changed for SSH user {}", name));
            }
            SshUsersCommands::Remove { cam, name, yes, dry_run } => {
                let client = cam.client()?;
                let accounts = ssh::list_users(&client)?;
                let names = account_names(&accounts);
                // An answer in a shape this does not know gives no names; then the camera decides.
                if !names.is_empty() && !names.contains(&name.as_str()) {
                    anyhow::bail!("no SSH account '{}' on {} (accounts: {})", name, cam.host, names.join(", "));
                }
                if dry_run {
                    format::output(&serde_json::json!({"account": name, "ssh": true, "performed": false}), cam.plain);
                    return Ok(());
                }
                confirm(&format!("Removing SSH account '{}' from {} cannot be undone", name, cam.host), yes)?;
                ssh::remove_user(&client, &name)?;
                format::ok_msg(&format!("SSH user {} removed", name));
            }
        }
        Ok(())
    }
}
