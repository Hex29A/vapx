use clap::{Args, Subcommand, ValueEnum};
use serde_json::json;

use crate::config::credentials::resolve;
use crate::output::format;
use crate::cmd::{make_client, CameraArgs};
use crate::vapix::{acap_install, applications};
use crate::vapix::client::VapixClient;

#[derive(Args)]
pub struct AcapCmd {
    #[command(subcommand)]
    pub command: AcapCommands,
}

/// Installs take longer than ordinary requests: the package is uploaded, verified and unpacked.
const INSTALL_TIMEOUT_SECS: u64 = 600;

#[derive(Clone, Copy, ValueEnum)]
pub enum UnsignedArg {
    /// Show whether unsigned apps may be installed
    Status,
    /// Allow unsigned apps (needs --yes)
    On,
    /// Refuse unsigned apps (the default from AXIS OS 12)
    Off,
}

/// The package arguments shared by install and upgrade.
#[derive(Args, Clone)]
pub struct InstallArgs {
    /// The .eap package: a file or an https URL. Give it once per architecture
    /// (aarch64, armv7hf) and each camera gets the one that fits
    #[arg(long = "eap", required = true)]
    pub eap: Vec<String>,

    /// Install even if the package is older than the installed version
    #[arg(long)]
    pub allow_downgrade: bool,

    /// Start the app afterwards if it is not running (a new install is not started otherwise)
    #[arg(long)]
    pub start: bool,

    /// Show what would happen (install, upgrade, reinstall or downgrade) without changing anything
    #[arg(long)]
    pub dry_run: bool,
}

impl InstallArgs {
    fn run(&self, cam: &CameraArgs, upgrade_only: bool) -> anyhow::Result<()> {
        let loaded = self.eap.iter().map(|s| acap_install::load(s)).collect::<anyhow::Result<Vec<_>>>()?;
        let (creds, host) = cam.resolve()?;
        let client = make_client(&host, creds, Some(cam.timeout.unwrap_or(INSTALL_TIMEOUT_SECS)));
        let opts = acap_install::Options {
            allow_downgrade: self.allow_downgrade,
            upgrade_only,
            start: self.start,
            dry_run: self.dry_run,
        };
        let out = acap_install::install(&client, &loaded, opts)?;
        format::output(&out, cam.plain);
        Ok(())
    }
}

#[derive(Subcommand)]
pub enum AcapCommands {
    /// Install an application from a .eap package, or replace the installed one with the same
    /// name. Says first whether it is an install, upgrade, reinstall or downgrade
    Install {
        #[command(flatten)]
        cam: CameraArgs,
        #[command(flatten)]
        args: InstallArgs,
    },
    /// Like install, but only replaces an app that is already installed
    Upgrade {
        #[command(flatten)]
        cam: CameraArgs,
        #[command(flatten)]
        args: InstallArgs,
    },
    /// Install (or with --remove, remove) the license key of an installed application
    License {
        #[command(flatten)]
        cam: CameraArgs,
        /// The application's package name (see `vapx acap list`)
        #[arg(long)]
        app: String,
        /// The license key file
        #[arg(long, required_unless_present = "remove")]
        key: Option<std::path::PathBuf>,
        /// Remove the license key instead
        #[arg(long, conflicts_with = "key")]
        remove: bool,
    },
    /// Whether the camera installs unsigned applications (AllowUnsigned, AXIS OS 11.2 and later)
    Unsigned {
        /// status, on or off
        #[arg(value_enum)]
        mode: UnsignedArg,
        #[command(flatten)]
        cam: CameraArgs,
        /// Needed to allow unsigned apps
        #[arg(short, long)]
        yes: bool,
    },
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
            AcapCommands::Install { cam, args } => args.run(&cam, false)?,
            AcapCommands::Upgrade { cam, args } => args.run(&cam, true)?,
            AcapCommands::License { cam, app, key, remove } => {
                let client = cam.client()?;
                if acap_install::find_app(&client, &app)?.is_none() {
                    anyhow::bail!("{app} is not installed on this camera (see `vapx acap list`)");
                }
                if remove {
                    acap_install::remove_license(&client, &app)?;
                } else {
                    let path = key.expect("clap requires --key without --remove");
                    let data = std::fs::read(&path).map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "license.xml".into());
                    acap_install::upload_license(&client, &app, &data, &name)?;
                }
                let after = acap_install::find_app(&client, &app)?;
                format::output(
                    &json!({
                        "app": app,
                        "action": if remove { "removed" } else { "installed" },
                        "license": after.as_ref().map(|a| a.license.clone()),
                    }),
                    cam.plain,
                );
            }
            AcapCommands::Unsigned { mode, cam, yes } => {
                let client = cam.client()?;
                let before = acap_install::allow_unsigned(&client)?;
                let out = match mode {
                    UnsignedArg::Status => json!({"allowUnsigned": before}),
                    UnsignedArg::On if !yes => json!({
                        "performed": false,
                        "allowUnsigned": before,
                        "note": "Nothing was changed. Unsigned apps have not been checked by their vendor; run again with --yes to allow them.",
                    }),
                    UnsignedArg::On => json!({"performed": true, "before": before, "allowUnsigned": acap_install::set_allow_unsigned(&client, true)?}),
                    UnsignedArg::Off => json!({"performed": true, "before": before, "allowUnsigned": acap_install::set_allow_unsigned(&client, false)?}),
                };
                format::output(&out, cam.plain);
            }
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
