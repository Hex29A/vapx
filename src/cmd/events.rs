use clap::Args;

use crate::config::credentials::resolve;
use crate::vapix::client::VapixClient;
use crate::vapix::events;

#[derive(Args)]
pub struct EventsCmd {
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

    /// ONVIF topic filter (e.g. "tns1:Device/tnsaxis:IO/VirtualPort")
    #[arg(short, long)]
    pub topic: Option<String>,

    /// Max number of events to receive (0 = unlimited)
    #[arg(short = 'n', long, default_value_t = 0)]
    pub count: u64,

    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

impl EventsCmd {
    pub fn run(self) -> anyhow::Result<()> {
        let (creds, resolved_host) = resolve(
            &self.host,
            self.user.as_deref(),
            self.pass.as_deref(),
            self.port,
            self.insecure,
        )?;

        let timeout = self.timeout.unwrap_or(creds.timeout);
        let client = VapixClient::new(&resolved_host, creds.port, creds.clone(), timeout);

        let mut received = 0u64;
        let max_count = self.count;

        events::stream_events(
            &client,
            &creds,
            &resolved_host,
            self.topic.as_deref(),
            |event| {
                // Extract the notification for cleaner output
                let notification = event
                    .pointer("/params/notification")
                    .unwrap_or(event);

                // Print each event as a standalone JSON object (one per line for streaming)
                println!("{}", serde_json::to_string(notification).unwrap_or_default());

                received += 1;
                if max_count > 0 && received >= max_count {
                    return false; // stop
                }
                true // continue
            },
        )?;

        Ok(())
    }
}
