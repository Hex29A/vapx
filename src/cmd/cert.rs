use clap::{Args, Subcommand};

use crate::output::format;
use crate::vapix::certs;

#[derive(Args)]
pub struct CertCmd {
    #[command(subcommand)]
    pub command: CertCommands,
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
pub enum CertCommands {
    /// List installed certificates
    List {
        #[command(flatten)]
        cam: CameraArgs,
    },
    /// Show certificate details
    Info {
        #[command(flatten)]
        cam: CameraArgs,

        /// Certificate ID
        #[arg(long)]
        id: String,
    },
    /// Generate a self-signed certificate
    SelfSign {
        #[command(flatten)]
        cam: CameraArgs,

        /// Common name (e.g., camera hostname or IP)
        #[arg(long)]
        cn: String,

        /// Validity in days (default: 365)
        #[arg(long, default_value_t = 365)]
        days: u32,
    },
    /// Generate a Certificate Signing Request (CSR)
    Csr {
        #[command(flatten)]
        cam: CameraArgs,

        /// Common name
        #[arg(long)]
        cn: String,

        /// Country code (e.g., SE)
        #[arg(long)]
        country: Option<String>,

        /// Organization name
        #[arg(long)]
        org: Option<String>,
    },
    /// Remove a certificate
    Remove {
        #[command(flatten)]
        cam: CameraArgs,

        /// Certificate ID to remove
        #[arg(long)]
        id: String,
    },
}

impl CertCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            CertCommands::List { cam } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&host, creds, cam.timeout);
                let resp = certs::list(&client)?;
                let data = resp.get("data").unwrap_or(&resp);
                if cam.plain {
                    format::plain(data);
                } else {
                    format::ok(data);
                }
            }
            CertCommands::Info { cam, id } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&host, creds, cam.timeout);
                let resp = certs::info(&client, &id)?;
                let data = resp.get("data").unwrap_or(&resp);
                if cam.plain {
                    format::plain(data);
                } else {
                    format::ok(data);
                }
            }
            CertCommands::SelfSign { cam, cn, days } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&host, creds, cam.timeout);
                let resp = certs::create_self_signed(&client, &cn, days)?;
                let data = resp.get("data").unwrap_or(&resp);
                format::ok(data);
            }
            CertCommands::Csr { cam, cn, country, org } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&host, creds, cam.timeout);
                let resp = certs::create_csr(
                    &client,
                    &cn,
                    country.as_deref(),
                    org.as_deref(),
                )?;
                let data = resp.get("data").unwrap_or(&resp);
                format::ok(data);
            }
            CertCommands::Remove { cam, id } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&host, creds, cam.timeout);
                certs::remove(&client, &id)?;
                format::ok_msg(&format!("Certificate {} removed", id));
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
