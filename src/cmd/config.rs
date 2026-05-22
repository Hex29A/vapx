use clap::{Args, Subcommand};

use crate::config::cameras;
use crate::output::format;

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
                    Some(p) => {
                        format::ok(&serde_json::json!({"path": p.display().to_string()}));
                    }
                    None => {
                        format::err_json("CONFIG_NOT_FOUND", "No config file found");
                    }
                }
            }
            ConfigCommands::Check => {
                match cameras::config_path() {
                    Some(p) => {
                        match cameras::load_cameras() {
                            Ok(Some(config)) => {
                                let mut warnings: Vec<String> = Vec::new();
                                for (name, entry) in &config.cameras {
                                    if entry.pass.as_deref() == Some("") {
                                        warnings.push(format!("Camera '{}' has empty password (env var not set?)", name));
                                    }
                                }
                                format::ok(&serde_json::json!({
                                    "path": p.display().to_string(),
                                    "cameras": config.cameras.len(),
                                    "groups": config.groups.keys().collect::<Vec<_>>(),
                                    "warnings": warnings,
                                }));
                            }
                            Ok(None) => {
                                format::err_json("CONFIG_EMPTY", "No config loaded");
                            }
                            Err(e) => {
                                format::err_json("CONFIG_INVALID", &e.to_string());
                            }
                        }
                    }
                    None => {
                        format::err_json("CONFIG_NOT_FOUND", "No config file found");
                    }
                }
            }
            ConfigCommands::List => {
                match cameras::load_cameras()? {
                    Some(config) => {
                        let cameras: Vec<serde_json::Value> = config.cameras.iter().map(|(name, entry)| {
                            let user = config.effective_user(entry).unwrap_or_else(|| "-".into());
                            let proto = if config.effective_https(entry) { "https" } else { "http" };
                            serde_json::json!({
                                "name": name,
                                "host": entry.host,
                                "protocol": proto,
                                "user": user,
                            })
                        }).collect();
                        format::ok(&cameras);
                    }
                    None => {
                        format::err_json("CONFIG_NOT_FOUND", "No config file found");
                    }
                }
            }
            ConfigCommands::Init => {
                let target = dirs::config_dir()
                    .map(|d| d.join("vapx").join("cameras.yaml"))
                    .unwrap_or_else(|| std::path::PathBuf::from("cameras.yaml"));

                if target.exists() {
                    format::err_json("CONFIG_EXISTS", &format!("Config already exists: {}", target.display()));
                }

                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                std::fs::write(&target, TEMPLATE_CONFIG)?;
                format::ok_msg(&format!("Created: {}", target.display()));
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
