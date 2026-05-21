use clap::Args;

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::firmware;

#[derive(Args)]
pub struct FwCmd {
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

impl FwCmd {
    pub fn run(self) -> anyhow::Result<()> {
        let (creds, resolved_host) = resolve(
            &self.host,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        )?;

        let timeout = self.timeout.unwrap_or(120);
        let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
        let resp = firmware::status(&client)?;

        let output = resp
            .get("data")
            .unwrap_or(&resp);

        if self.plain {
            format::plain(output);
        } else {
            format::json(output);
        }

        Ok(())
    }
}
