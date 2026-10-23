use clap::{Args, Subcommand};

use crate::cmd::CameraArgs;
use crate::output::format;
use crate::vapix::certs::{self, Kind, SelfSignOptions};

#[derive(Args)]
pub struct CertCmd {
    #[command(subcommand)]
    pub command: CertCommands,
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
    /// Generate a self-signed certificate (the camera makes the key pair)
    SelfSign {
        #[command(flatten)]
        cam: CameraArgs,

        /// Common name (e.g., camera hostname or IP)
        #[arg(long)]
        cn: String,

        /// Validity in days (default: 365)
        #[arg(long, default_value_t = 365)]
        days: u32,

        /// Name to store the certificate under (AXIS OS 12+; default: the common name)
        #[arg(long)]
        alias: Option<String>,

        /// Key type, e.g. EC-P256 or RSA-2048 (AXIS OS 12+; default: the camera's choice)
        #[arg(long)]
        key_type: Option<String>,

        /// Keystore for the private key (AXIS OS 12+; default: the camera's default, see 'cert list')
        #[arg(long)]
        keystore: Option<String>,
    },
    /// Generate a Certificate Signing Request (CSR)
    ///
    /// On AXIS OS 12+ the CSR is made for the key of an existing certificate (--alias; 'cert self-sign'
    /// creates one with a new key) and --cn, --country and --org replace its subject when given.
    /// Older cameras make a key and a CSR in one step and need --cn.
    Csr {
        #[command(flatten)]
        cam: CameraArgs,

        /// Alias of the existing certificate whose key is used (AXIS OS 12+)
        #[arg(long)]
        alias: Option<String>,

        /// Common name
        #[arg(long)]
        cn: Option<String>,

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

        /// Certificate alias (AXIS OS 12+) or ID to remove
        #[arg(long)]
        id: String,

        /// Which collection the alias is in, when it exists as both a certificate and a CA certificate
        #[arg(long, value_parser = ["certificate", "ca"])]
        kind: Option<String>,
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
                format::output(data, cam.plain);
            }
            CertCommands::Info { cam, id } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&host, creds, cam.timeout);
                let resp = certs::info(&client, &id)?;
                let data = resp.get("data").unwrap_or(&resp);
                format::output(data, cam.plain);
            }
            CertCommands::SelfSign { cam, cn, days, alias, key_type, keystore } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&host, creds, cam.timeout);
                let opts = SelfSignOptions { key_type: key_type.as_deref(), keystore: keystore.as_deref() };
                let resp = certs::create_self_signed(&client, &cn, alias.as_deref().unwrap_or(&cn), days, &opts)?;
                let data = resp.get("data").unwrap_or(&resp);
                format::ok(data);
            }
            CertCommands::Csr { cam, alias, cn, country, org } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&host, creds, cam.timeout);
                let resp = certs::create_csr(
                    &client,
                    alias.as_deref(),
                    cn.as_deref(),
                    country.as_deref(),
                    org.as_deref(),
                )?;
                let data = resp.get("data").unwrap_or(&resp);
                format::ok(data);
            }
            CertCommands::Remove { cam, id, kind } => {
                let (creds, host) = resolve_cam(&cam)?;
                let client = crate::cmd::make_client(&host, creds, cam.timeout);
                let kind = kind.as_deref().map(|k| if k == "ca" { Kind::Ca } else { Kind::Certificate });
                match certs::remove(&client, &id, kind)? {
                    Some(Kind::Ca) => format::ok_msg(&format!("CA certificate {} removed", id)),
                    Some(Kind::Certificate) => format::ok_msg(&format!("Certificate {} removed", id)),
                    None => format::ok_msg(&format!("Certificate {} removed", id)),
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
