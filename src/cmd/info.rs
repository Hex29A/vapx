use clap::Args;

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::device;

#[derive(Args)]
pub struct InfoCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,

    /// Username (overrides cameras.yaml)
    #[arg(short, long, env = "VAPX_USER")]
    pub user: Option<String>,

    /// Password (overrides cameras.yaml)
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

    /// Only show specific properties (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub props: Option<Vec<String>>,
}

impl InfoCmd {
    pub fn run(self) -> anyhow::Result<()> {
        let (creds, resolved_host) = resolve(
            &self.host,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        )?;

        let client = VapixClient::new(&resolved_host, creds.port, creds, 10);

        let resp = if let Some(ref props) = self.props {
            let prop_refs: Vec<&str> = props.iter().map(|s| s.as_str()).collect();
            device::get_properties(&client, &prop_refs)?
        } else {
            device::get_all_properties(&client)?
        };

        // Extract just the propertyList for cleaner output
        let output = resp
            .get("data")
            .and_then(|d| d.get("propertyList"))
            .unwrap_or(&resp);

        if self.plain {
            format::plain(output);
        } else {
            format::json(output);
        }

        Ok(())
    }
}
