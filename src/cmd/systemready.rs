use clap::Args;

use crate::config::credentials::Credentials;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::systemready;

#[derive(Args)]
pub struct SystemreadyCmd {
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

    /// Seconds the device may hold the request open while booting
    #[arg(long, default_value = "10")]
    pub wait: u64,

    /// Poll until the device reports ready, up to this many seconds
    #[arg(long)]
    pub until_ready: Option<u64>,

    /// Only accept a reply from a boot other than this one.
    ///
    /// A camera that has been told to reboot keeps answering "ready" for a few
    /// seconds before it actually goes down, so polling straight after a
    /// reboot or factory default otherwise returns the *old* state. Pass the
    /// bootid read before the reboot to wait for the device that comes back.
    #[arg(long)]
    pub after_bootid: Option<String>,
}

impl SystemreadyCmd {
    pub fn run(self) -> anyhow::Result<()> {
        let client = self.build_client()?;

        let deadline = self
            .until_ready
            .map(|s| std::time::Instant::now() + std::time::Duration::from_secs(s));

        let state = loop {
            match systemready::query(&client, self.wait) {
                Ok(s) => {
                    // While waiting for a specific boot, a reply carrying the
                    // old bootid is the pre-reboot device still answering.
                    let stale = match (&self.after_bootid, &s.bootid) {
                        (Some(old), Some(current)) => old == current,
                        (Some(_), None) => true, // can't tell — keep waiting
                        (None, _) => false,
                    };
                    if !stale && (s.systemready || deadline.is_none()) {
                        break s;
                    }
                    if stale {
                        tracing::debug!("still the pre-reboot boot; waiting");
                    }
                }
                // A rebooting camera refuses connections; that is expected
                // while polling, so keep waiting until the deadline.
                Err(e) => {
                    if deadline.is_none() {
                        return Err(e);
                    }
                    tracing::debug!("systemready not answering yet: {}", e);
                }
            }

            match deadline {
                Some(d) if std::time::Instant::now() < d => {
                    std::thread::sleep(std::time::Duration::from_secs(5));
                }
                Some(_) => anyhow::bail!(
                    "Camera did not become ready within {}s",
                    self.until_ready.unwrap()
                ),
                None => unreachable!("handled above"),
            }
        };

        let out = serde_json::json!({
            "systemready": state.systemready,
            "needsetup": state.needsetup,
            "passphrase_policy": state.passphrase_policy.as_str(),
            "uptime": state.uptime,
            "preview_mode": state.preview_mode,
            "bootid": state.bootid,
            // Everything the device sent, including fields newer firmware may
            // add that this build does not model yet.
            "device_response": state.raw,
        });

        if self.plain {
            println!(
                "systemready={} needsetup={} policy={}",
                state.systemready,
                state.needsetup,
                state.passphrase_policy.as_str()
            );
        } else {
            format::ok(&out);
        }

        Ok(())
    }

    /// Build a client that works before any account exists.
    ///
    /// systemready.cgi is the one endpoint that answers unauthenticated, so
    /// this must not go through the normal credential resolution — that would
    /// prompt or fail on a factory-default camera, which is exactly the case
    /// this command exists to detect. Credentials are used when supplied, and
    /// the host is still resolved through cameras.yaml when it names a
    /// configured camera.
    fn build_client(&self) -> anyhow::Result<VapixClient> {
        let (creds, host) = match crate::cmd::resolve_cam(
            &self.host,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        ) {
            Ok(pair) => pair,
            Err(_) => {
                // No credentials anywhere — fall back to an anonymous client,
                // resolving the host through the config if it is known there.
                let host = crate::config::cameras::load_cameras()
                    .ok()
                    .flatten()
                    .and_then(|c| c.find(&self.host).map(|(_, e)| e.host.clone()))
                    .unwrap_or_else(|| self.host.clone());
                (
                    Credentials {
                        user: String::new(),
                        pass: String::new(),
                        https: false,
                        verify_ssl: !self.insecure,
                        port: self.port.unwrap_or(80),
                        timeout: 10,
                    },
                    host,
                )
            }
        };

        Ok(crate::cmd::make_client(&host, creds, self.timeout))
    }
}
