use clap::{Args, Subcommand};

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

        /// Password for the account
        #[arg(long)]
        pwd: String,

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

        /// New password
        #[arg(long)]
        pwd: String,
    },
    /// Remove an SSH account
    Remove {
        #[command(flatten)]
        cam: CameraArgs,

        /// Account name to remove
        #[arg(long)]
        name: String,
    },
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
                ssh::add_user(&cam.client()?, &name, &pwd, &comment)?;
                format::ok_msg(&format!("SSH user {} created", name));
            }
            SshUsersCommands::Passwd { cam, name, pwd } => {
                ssh::set_password(&cam.client()?, &name, &pwd)?;
                format::ok_msg(&format!("Password changed for SSH user {}", name));
            }
            SshUsersCommands::Remove { cam, name } => {
                ssh::remove_user(&cam.client()?, &name)?;
                format::ok_msg(&format!("SSH user {} removed", name));
            }
        }
        Ok(())
    }
}
