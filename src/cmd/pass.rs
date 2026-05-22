use clap::Args;

use crate::config::credentials::resolve;
use crate::vapix::client::VapixClient;
use crate::vapix::users;

#[derive(Args)]
pub struct PassCmd {
    /// Camera IP, hostname, or name from cameras.yaml
    pub host: String,
    /// Account name whose password to change (defaults to authenticating user)
    #[arg(long)]
    pub name: Option<String>,
    /// New password
    #[arg(long)]
    pub pwd: String,
    #[arg(short, long, env = "VAPX_USER")]
    pub user: Option<String>,
    #[arg(short, long, env = "VAPX_PASS")]
    pub pass: Option<String>,
    #[arg(short = 'k', long)]
    pub insecure: bool,
    #[arg(long)]
    pub port: Option<u16>,
    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

impl PassCmd {
    pub fn run(self) -> anyhow::Result<()> {
        let (creds, resolved_host) = resolve(
            &self.host,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        )?;
        let timeout = self.timeout.unwrap_or(creds.timeout);
        let target_user = self.name.unwrap_or_else(|| creds.user.clone());
        let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
        let result = users::update(&client, &target_user, &self.pwd)?;
        crate::output::format::ok_msg(result.trim());
        Ok(())
    }
}
