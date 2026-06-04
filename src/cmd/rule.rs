use clap::{Args, Subcommand};

use crate::cmd::CameraArgs;
use crate::output::format;
use crate::vapix::rules;

#[derive(Args)]
pub struct RuleCmd {
    #[command(subcommand)]
    pub command: RuleCommands,
}

#[derive(Subcommand)]
pub enum RuleCommands {
    /// List all action rules
    List {
        #[command(flatten)]
        cam: CameraArgs,
    },
    /// Show details for a specific rule
    Info {
        #[command(flatten)]
        cam: CameraArgs,

        /// Rule ID
        #[arg(long)]
        id: String,
    },
    /// Remove a rule
    Remove {
        #[command(flatten)]
        cam: CameraArgs,

        /// Rule ID to remove
        #[arg(long)]
        id: String,
    },
    /// Enable a rule
    Enable {
        #[command(flatten)]
        cam: CameraArgs,

        /// Rule ID to enable
        #[arg(long)]
        id: String,
    },
    /// Disable a rule
    Disable {
        #[command(flatten)]
        cam: CameraArgs,

        /// Rule ID to disable
        #[arg(long)]
        id: String,
    },
    /// List available action templates
    Templates {
        #[command(flatten)]
        cam: CameraArgs,
    },
    /// List available recipient templates
    Recipients {
        #[command(flatten)]
        cam: CameraArgs,
    },
}

impl RuleCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            RuleCommands::List { cam } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&host, creds, cam.timeout);
                let resp = rules::list_rules(&client)?;
                let data = resp.get("data").unwrap_or(&resp);
                format::output(data, cam.plain);
            }
            RuleCommands::Info { cam, id } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&host, creds, cam.timeout);
                let resp = rules::get_rule(&client, &id)?;
                let data = resp.get("data").unwrap_or(&resp);
                format::output(data, cam.plain);
            }
            RuleCommands::Remove { cam, id } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&host, creds, cam.timeout);
                rules::remove_rule(&client, &id)?;
                format::ok_msg(&format!("Rule {} removed", id));
            }
            RuleCommands::Enable { cam, id } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&host, creds, cam.timeout);
                rules::set_rule_enabled(&client, &id, true)?;
                format::ok_msg(&format!("Rule {} enabled", id));
            }
            RuleCommands::Disable { cam, id } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&host, creds, cam.timeout);
                rules::set_rule_enabled(&client, &id, false)?;
                format::ok_msg(&format!("Rule {} disabled", id));
            }
            RuleCommands::Templates { cam } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&host, creds, cam.timeout);
                let resp = rules::list_templates(&client)?;
                let data = resp.get("data").unwrap_or(&resp);
                format::output(data, cam.plain);
            }
            RuleCommands::Recipients { cam } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&host, creds, cam.timeout);
                let resp = rules::list_recipients(&client)?;
                let data = resp.get("data").unwrap_or(&resp);
                format::output(data, cam.plain);
            }
        }
        Ok(())
    }
}

fn resolve_cam(cam: &CameraArgs) -> anyhow::Result<(crate::config::credentials::Credentials, String)> {
    crate::cmd::resolve_cam(
        &cam.host,
        cam.user.as_deref(),
        cam.pass.as_deref(),
        cam.port,
        cam.insecure,
    )
}
