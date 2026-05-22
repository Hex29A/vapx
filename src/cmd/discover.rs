use clap::Args;

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::discover;

#[derive(Args)]
pub struct DiscoverCmd {
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

impl DiscoverCmd {
    pub fn run(self) -> anyhow::Result<()> {
        let (creds, resolved_host) = resolve(
            &self.host,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        )?;

        let timeout = self.timeout.unwrap_or(creds.timeout);
        let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
        let resp = discover::get_api_list(&client)?;

        let output = resp
            .pointer("/data/apiList")
            .unwrap_or(&resp);

        if self.plain {
            if let Some(arr) = output.as_array() {
                for api in arr {
                    let id = api.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                    let version = api.get("version").and_then(|v| v.as_str()).unwrap_or("?");
                    let name = api.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    println!("{:<40} v{:<8} {}", id, version, name);
                }
            } else {
                format::plain(output);
            }
        } else {
            format::ok(output);
        }

        Ok(())
    }
}
