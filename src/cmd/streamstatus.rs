use clap::Args;

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::streamstatus;

#[derive(Args)]
pub struct StreamstatusCmd {
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

impl StreamstatusCmd {
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
        let text = streamstatus::get_stream_status(&client)?;

        if self.plain {
            println!("{}", text);
        } else {
            // Try parsing as JSON first (from streamstatus.cgi)
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(data) = json.get("data") {
                    format::ok(data);
                } else {
                    format::ok(&json);
                }
            } else {
                // param.cgi key=value format
                let map = crate::cmd::param_to_json(&text);
                format::ok(&map);
            }
        }

        Ok(())
    }
}
