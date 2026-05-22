use clap::{Args, Subcommand};

use crate::config::credentials::resolve;
use crate::output::format;
use crate::vapix::applications;
use crate::vapix::client::VapixClient;

#[derive(Args)]
pub struct AcapCmd {
    #[command(subcommand)]
    pub command: AcapCommands,
}

#[derive(Subcommand)]
pub enum AcapCommands {
    /// List installed applications
    List {
        /// Camera IP, hostname, or name from cameras.yaml
        host: String,
        #[arg(short, long, env = "VAPX_USER")]
        user: Option<String>,
        #[arg(short, long, env = "VAPX_PASS")]
        pass: Option<String>,
        #[arg(short = 'k', long)]
        insecure: bool,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long)]
        plain: bool,
        /// Request timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Start an application
    Start {
        /// Camera IP, hostname, or name from cameras.yaml
        host: String,
        /// Application package name
        #[arg(long)]
        package: String,
        #[arg(short, long, env = "VAPX_USER")]
        user: Option<String>,
        #[arg(short, long, env = "VAPX_PASS")]
        pass: Option<String>,
        #[arg(short = 'k', long)]
        insecure: bool,
        #[arg(long)]
        port: Option<u16>,
        /// Request timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Stop an application
    Stop {
        /// Camera IP, hostname, or name from cameras.yaml
        host: String,
        /// Application package name
        #[arg(long)]
        package: String,
        #[arg(short, long, env = "VAPX_USER")]
        user: Option<String>,
        #[arg(short, long, env = "VAPX_PASS")]
        pass: Option<String>,
        #[arg(short = 'k', long)]
        insecure: bool,
        #[arg(long)]
        port: Option<u16>,
        /// Request timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Restart an application
    Restart {
        /// Camera IP, hostname, or name from cameras.yaml
        host: String,
        /// Application package name
        #[arg(long)]
        package: String,
        #[arg(short, long, env = "VAPX_USER")]
        user: Option<String>,
        #[arg(short, long, env = "VAPX_PASS")]
        pass: Option<String>,
        #[arg(short = 'k', long)]
        insecure: bool,
        #[arg(long)]
        port: Option<u16>,
        /// Request timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Remove an application
    Remove {
        /// Camera IP, hostname, or name from cameras.yaml
        host: String,
        /// Application package name
        #[arg(long)]
        package: String,
        #[arg(short, long, env = "VAPX_USER")]
        user: Option<String>,
        #[arg(short, long, env = "VAPX_PASS")]
        pass: Option<String>,
        #[arg(short = 'k', long)]
        insecure: bool,
        #[arg(long)]
        port: Option<u16>,
        /// Request timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,
    },
}

impl AcapCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            AcapCommands::List {
                host,
                user,
                pass,
                insecure,
                port,
                plain,
                timeout,
            } => {
                let (creds, resolved_host) =
                    resolve(&host, user.as_deref(), pass.as_deref(), port, insecure)?;
                let t = timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds, t);
                let apps = applications::list_applications(&client)?;

                if plain {
                    for app in &apps {
                        println!(
                            "{:<24} {:<8} {:<10} v{} ({})",
                            app.name, app.status, app.license, app.version, app.vendor
                        );
                    }
                    if apps.is_empty() {
                        println!("No applications installed.");
                    }
                } else {
                    format::ok(&apps);
                }
            }
            AcapCommands::Start {
                host,
                package,
                user,
                pass,
                insecure,
                port,
                timeout,
            } => {
                let (creds, resolved_host) =
                    resolve(&host, user.as_deref(), pass.as_deref(), port, insecure)?;
                let t = timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds, t);
                applications::control(&client, "start", &package)?;
                format::ok_msg(&format!("Started: {}", package));
            }
            AcapCommands::Stop {
                host,
                package,
                user,
                pass,
                insecure,
                port,
                timeout,
            } => {
                let (creds, resolved_host) =
                    resolve(&host, user.as_deref(), pass.as_deref(), port, insecure)?;
                let t = timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds, t);
                applications::control(&client, "stop", &package)?;
                format::ok_msg(&format!("Stopped: {}", package));
            }
            AcapCommands::Restart {
                host,
                package,
                user,
                pass,
                insecure,
                port,
                timeout,
            } => {
                let (creds, resolved_host) =
                    resolve(&host, user.as_deref(), pass.as_deref(), port, insecure)?;
                let t = timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds, t);
                applications::control(&client, "restart", &package)?;
                format::ok_msg(&format!("Restarted: {}", package));
            }
            AcapCommands::Remove {
                host,
                package,
                user,
                pass,
                insecure,
                port,
                timeout,
            } => {
                let (creds, resolved_host) =
                    resolve(&host, user.as_deref(), pass.as_deref(), port, insecure)?;
                let t = timeout.unwrap_or(creds.timeout);
                let client = VapixClient::new(&resolved_host, creds.port, creds, t);
                applications::control(&client, "remove", &package)?;
                format::ok_msg(&format!("Removed: {}", package));
            }
        }
        Ok(())
    }
}
