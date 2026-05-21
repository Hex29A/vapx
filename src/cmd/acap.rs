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
            } => {
                let (creds, resolved_host) =
                    resolve(&host, user.as_deref(), pass.as_deref(), port, insecure)?;
                let client = VapixClient::new(&resolved_host, creds.port, creds, 10);
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
                    format::json(&apps);
                }
            }
            AcapCommands::Start {
                host,
                package,
                user,
                pass,
                insecure,
                port,
            } => {
                let (creds, resolved_host) =
                    resolve(&host, user.as_deref(), pass.as_deref(), port, insecure)?;
                let client = VapixClient::new(&resolved_host, creds.port, creds, 10);
                applications::control(&client, "start", &package)?;
                println!("Started: {}", package);
            }
            AcapCommands::Stop {
                host,
                package,
                user,
                pass,
                insecure,
                port,
            } => {
                let (creds, resolved_host) =
                    resolve(&host, user.as_deref(), pass.as_deref(), port, insecure)?;
                let client = VapixClient::new(&resolved_host, creds.port, creds, 10);
                applications::control(&client, "stop", &package)?;
                println!("Stopped: {}", package);
            }
            AcapCommands::Restart {
                host,
                package,
                user,
                pass,
                insecure,
                port,
            } => {
                let (creds, resolved_host) =
                    resolve(&host, user.as_deref(), pass.as_deref(), port, insecure)?;
                let client = VapixClient::new(&resolved_host, creds.port, creds, 10);
                applications::control(&client, "restart", &package)?;
                println!("Restarted: {}", package);
            }
            AcapCommands::Remove {
                host,
                package,
                user,
                pass,
                insecure,
                port,
            } => {
                let (creds, resolved_host) =
                    resolve(&host, user.as_deref(), pass.as_deref(), port, insecure)?;
                let client = VapixClient::new(&resolved_host, creds.port, creds, 10);
                applications::control(&client, "remove", &package)?;
                println!("Removed: {}", package);
            }
        }
        Ok(())
    }
}
