use clap::{Args, Subcommand};

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::client::VapixClient;

#[derive(Args)]
pub struct LogCmd {
    #[command(subcommand)]
    pub command: LogCommands,
}

#[derive(Subcommand)]
pub enum LogCommands {
    /// Show system log
    System {
        #[command(flatten)]
        cam: CameraArgs,
    },
    /// Show access log
    Access {
        #[command(flatten)]
        cam: CameraArgs,
    },
    /// Show server report (full diagnostic)
    Report {
        #[command(flatten)]
        cam: CameraArgs,
    },
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

    /// Number of lines from the end (newest first)
    #[arg(short = 'n', long)]
    pub tail: Option<usize>,

    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,
}

impl LogCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            LogCommands::System { cam } => {
                let (creds, resolved_host) = resolve(
                    &cam.host, cam.user.as_deref(), cam.pass.as_deref(),
                    cam.port, cam.insecure,
                )?;
                let timeout = cam.timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
                let text = client.get_text("/axis-cgi/systemlog.cgi", &[])?;
                output_log(&text, cam.tail);
            }
            LogCommands::Access { cam } => {
                let (creds, resolved_host) = resolve(
                    &cam.host, cam.user.as_deref(), cam.pass.as_deref(),
                    cam.port, cam.insecure,
                )?;
                let timeout = cam.timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
                let text = client.get_text("/axis-cgi/accesslog.cgi", &[])?;
                output_log(&text, cam.tail);
            }
            LogCommands::Report { cam } => {
                let (creds, resolved_host) = resolve(
                    &cam.host, cam.user.as_deref(), cam.pass.as_deref(),
                    cam.port, cam.insecure,
                )?;
                let timeout = cam.timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds, timeout);
                let text = client.get_text("/axis-cgi/serverreport.cgi", &[])?;
                output_log(&text, cam.tail);
            }
        }
        Ok(())
    }
}

fn output_log(text: &str, tail: Option<usize>) {
    let lines: Vec<&str> = text.lines().collect();
    let output_lines = if let Some(n) = tail {
        let start = lines.len().saturating_sub(n);
        &lines[start..]
    } else {
        &lines
    };

    format::ok(&serde_json::json!({
        "lines": output_lines.len(),
        "log": output_lines,
    }));
}
