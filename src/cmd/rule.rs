use clap::{Args, Subcommand};

use crate::output::format;
use crate::vapix::rules;

#[derive(Args)]
pub struct RuleCmd {
    #[command(subcommand)]
    pub command: RuleCommands,
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
                if cam.plain {
                    format::plain(data);
                } else {
                    format::ok(data);
                }
            }
            RuleCommands::Info { cam, id } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&host, creds, cam.timeout);
                let resp = rules::get_rule(&client, &id)?;
                let data = resp.get("data").unwrap_or(&resp);
                if cam.plain {
                    format::plain(data);
                } else {
                    format::ok(data);
                }
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
                if cam.plain {
                    format::plain(data);
                } else {
                    format::ok(data);
                }
            }
            RuleCommands::Recipients { cam } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&host, creds, cam.timeout);
                let resp = rules::list_recipients(&client)?;
                let data = resp.get("data").unwrap_or(&resp);
                if cam.plain {
                    format::plain(data);
                } else {
                    format::ok(data);
                }
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
