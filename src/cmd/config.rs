use clap::{Args, Subcommand};

use crate::config::cameras;

#[derive(Args)]
pub struct ConfigCmd {
    #[command(subcommand)]
    pub command: ConfigCommands,
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show resolved config file path
    Path,
    /// Validate config file
    Check,
    /// List all configured cameras
    List,
    /// Create a template config file
    Init,
}

impl ConfigCmd {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            ConfigCommands::Path => {
                match cameras::config_path() {
                    Some(p) => println!("{}", p.display()),
                    None => {
                        eprintln!("No config file found.");
                        eprintln!("Searched:");
                        eprintln!("  1. $VAPX_CONFIG");
                        eprintln!("  2. ./cameras.yaml");
                        if let Some(d) = dirs::config_dir() {
                            eprintln!("  3. {}/vapx/cameras.yaml", d.display());
                        }
                        std::process::exit(1);
                    }
                }
            }
            ConfigCommands::Check => {
                match cameras::config_path() {
                    Some(p) => {
                        println!("Config file: {}", p.display());
                        match cameras::load_cameras() {
                            Ok(Some(config)) => {
                                println!("OK: {} cameras configured", config.cameras.len());
                                if !config.groups.is_empty() {
                                    println!("Groups: {}", config.groups.keys().map(|k| k.as_str()).collect::<Vec<_>>().join(", "));
                                }
                                // Check for unresolved env vars
                                for (name, entry) in &config.cameras {
                                    if entry.pass.as_deref() == Some("") {
                                        eprintln!("WARNING: Camera '{}' has empty password (env var not set?)", name);
                                    }
                                }
                            }
                            Ok(None) => {
                                eprintln!("No config loaded");
                                std::process::exit(1);
                            }
                            Err(e) => {
                                eprintln!("ERROR: {}", e);
                                std::process::exit(1);
                            }
                        }
                    }
                    None => {
                        eprintln!("No config file found.");
                        std::process::exit(1);
                    }
                }
            }
            ConfigCommands::List => {
                match cameras::load_cameras()? {
                    Some(config) => {
                        for (name, entry) in &config.cameras {
                            let user = config.effective_user(entry).unwrap_or_else(|| "-".into());
                            let proto = if config.effective_https(entry) { "https" } else { "http" };
                            println!("{:<16} {:<16} {}  user={}", name, entry.host, proto, user);
                        }
                    }
                    None => {
                        eprintln!("No config file found.");
                        std::process::exit(1);
                    }
                }
            }
            ConfigCommands::Init => {
                let target = dirs::config_dir()
                    .map(|d| d.join("vapx").join("cameras.yaml"))
                    .unwrap_or_else(|| std::path::PathBuf::from("cameras.yaml"));

                if target.exists() {
                    eprintln!("Config already exists: {}", target.display());
                    std::process::exit(1);
                }

                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                std::fs::write(&target, TEMPLATE_CONFIG)?;
                println!("Created: {}", target.display());
            }
        }
        Ok(())
    }
}

const TEMPLATE_CONFIG: &str = r#"# vapx camera configuration
# Env vars: use ${VAR_NAME} for secrets, loaded from environment.

defaults:
  user: root
  https: false
  verify_ssl: false
  timeout: 10

cameras:
  # example:
  #   host: 192.168.1.100
  #   pass: "${EXAMPLE_PASS}"
  #   port: 80

groups: {}
  # home:
  #   - example
"#;
