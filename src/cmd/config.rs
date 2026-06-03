use clap::{Args, Subcommand};

use crate::config::cameras;
use crate::config::credentials;
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::device;

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
    /// Add a camera to config (with optional connectivity check)
    Add {
        /// Name for this camera in config
        #[arg(long)]
        name: String,
        /// Camera IP or hostname
        #[arg(long)]
        host: String,
        /// Username
        #[arg(short, long)]
        user: Option<String>,
        /// Password
        #[arg(short, long)]
        pass: Option<String>,
        /// Use HTTPS
        #[arg(long)]
        https: bool,
        /// Port number
        #[arg(long)]
        port: Option<u16>,
        /// Skip connectivity verification
        #[arg(long)]
        no_verify: bool,
    },
    /// Store a camera password in the OS keyring
    SetSecret {
        /// Camera name (as defined in cameras.yaml)
        name: String,
    },
    /// Retrieve a camera password from the OS keyring
    GetSecret {
        /// Camera name (as defined in cameras.yaml)
        name: String,
    },
    /// Remove a camera password from the OS keyring
    RemoveSecret {
        /// Camera name (as defined in cameras.yaml)
        name: String,
    },
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
            ConfigCommands::Add { name, host, user, pass, https, port, no_verify } => {
                // Verify connectivity unless --no-verify
                if !no_verify {
                    let cred_user = user.as_deref().unwrap_or("root");
                    let cred_pass = pass.as_deref().unwrap_or("");

                    if cred_pass.is_empty() {
                        anyhow::bail!("Password required for connectivity check. Use --pass or --no-verify to skip.");
                    }

                    let (creds, resolved) = credentials::resolve(
                        &host,
                        Some(cred_user),
                        Some(cred_pass),
                        port,
                        !https, // insecure if not https
                    )?;
                    let client = VapixClient::new(&resolved, creds.port, creds, 5);
                    let info = device::get_all_properties(&client)?;
                    let model = info
                        .pointer("/data/propertyList/ProdNbr")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    eprintln!("Verified: {} ({})", host, model);
                }

                // Build YAML entry and append to config file
                let config_path = cameras::config_path()
                    .or_else(|| dirs::config_dir().map(|d| d.join("vapx").join("cameras.yaml")))
                    .unwrap_or_else(|| std::path::PathBuf::from("cameras.yaml"));

                // Load existing to check for duplicates
                if let Some(config) = cameras::load_cameras()? {
                    if config.cameras.contains_key(&name) {
                        anyhow::bail!("Camera '{}' already exists in config", name);
                    }
                }

                // Build the entry lines
                let mut entry = format!("\n  {}:\n    host: {}\n", name, host);
                if let Some(ref u) = user {
                    entry.push_str(&format!("    user: {}\n", u));
                }
                if let Some(ref p) = pass {
                    entry.push_str(&format!("    pass: \"{}\"\n", p));
                }
                if https {
                    entry.push_str("    https: true\n");
                }
                if let Some(p) = port {
                    entry.push_str(&format!("    port: {}\n", p));
                }

                // Ensure the config file exists
                if !config_path.exists() {
                    if let Some(parent) = config_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&config_path, TEMPLATE_CONFIG)?;
                }

                // Insert entry into cameras: section (before groups: line, or at end)
                let content = std::fs::read_to_string(&config_path)?;
                let new_content = if let Some(pos) = content.find("\ngroups:") {
                    format!("{}{}{}", &content[..pos + 1], entry, &content[pos + 1..])
                } else {
                    format!("{}{}", content, entry)
                };
                std::fs::write(&config_path, new_content)?;

                format::ok_msg(&format!("Added camera '{}' ({})", name, host));
            }
            ConfigCommands::SetSecret { name } => {
                set_keyring_secret(&name)?;
            }
            ConfigCommands::GetSecret { name } => {
                get_keyring_secret(&name)?;
            }
            ConfigCommands::RemoveSecret { name } => {
                remove_keyring_secret(&name)?;
            }
        }
        Ok(())
    }
}

const TEMPLATE_CONFIG: &str = r#"# vapx camera configuration
# Docs: https://github.com/Hex29A/vapx#camerasyaml
# Env vars: use ${VAR_NAME} for secrets, loaded from environment.
# Keyring: use `vapx config set-secret <name>` to store passwords securely.
# Profiles: define named sets of defaults under profiles:, select with --profile.

defaults:
  user: root
  https: false
  verify_ssl: false
  timeout: 10          # seconds; increase for WAN cameras (e.g. 30)

cameras:
  # my-camera:
  #   host: 192.168.1.100
  #   pass: "${MY_CAMERA_PASS}"   # or plain text (not recommended)
  #   user: root                  # overrides defaults.user
  #   https: false
  #   verify_ssl: false
  #   port: 80
  #   timeout: 30                 # override for slow links
  #   fw_timeout: 600              # firmware upload timeout (default: 300s)
  #   enabled: true               # set false to skip in batch/watch/health

profiles: {}
  # wan:
  #   timeout: 30
  # secure:
  #   https: true
  #   verify_ssl: true

groups: {}
  # site-a:
  #   - my-camera
"#;

#[cfg(feature = "keyring")]
const KEYRING_SERVICE: &str = "vapx";

#[cfg(feature = "keyring")]
fn set_keyring_secret(name: &str) -> anyhow::Result<()> {
    let pass = rpassword::prompt_password(format!("Password for '{}': ", name))?;
    let entry = keyring::Entry::new(KEYRING_SERVICE, name)?;
    entry.set_password(&pass)?;
    format::ok_msg(&format!("Password stored in keyring for '{}'", name));
    Ok(())
}

#[cfg(not(feature = "keyring"))]
fn set_keyring_secret(_name: &str) -> anyhow::Result<()> {
    anyhow::bail!("Keyring support not compiled. Rebuild with: cargo build --features keyring");
}

#[cfg(feature = "keyring")]
fn get_keyring_secret(name: &str) -> anyhow::Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, name)?;
    match entry.get_password() {
        Ok(_) => format::ok_msg(&format!("Keyring entry exists for '{}'", name)),
        Err(keyring::Error::NoEntry) => {
            format::err_json("NOT_FOUND", &format!("No keyring entry for '{}'", name));
        }
        Err(e) => anyhow::bail!("Keyring error: {}", e),
    }
    Ok(())
}

#[cfg(not(feature = "keyring"))]
fn get_keyring_secret(_name: &str) -> anyhow::Result<()> {
    anyhow::bail!("Keyring support not compiled. Rebuild with: cargo build --features keyring");
}

#[cfg(feature = "keyring")]
fn remove_keyring_secret(name: &str) -> anyhow::Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, name)?;
    match entry.delete_credential() {
        Ok(()) => format::ok_msg(&format!("Removed keyring entry for '{}'", name)),
        Err(keyring::Error::NoEntry) => {
            format::err_json("NOT_FOUND", &format!("No keyring entry for '{}'", name));
        }
        Err(e) => anyhow::bail!("Keyring error: {}", e),
    }
    Ok(())
}

#[cfg(not(feature = "keyring"))]
fn remove_keyring_secret(_name: &str) -> anyhow::Result<()> {
    anyhow::bail!("Keyring support not compiled. Rebuild with: cargo build --features keyring");
}

/// Try to look up a password from the OS keyring (if feature enabled).
#[cfg(feature = "keyring")]
pub fn keyring_lookup(name: &str) -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, name)
        .ok()
        .and_then(|e| e.get_password().ok())
}

#[cfg(not(feature = "keyring"))]
pub fn keyring_lookup(_name: &str) -> Option<String> {
    None
}
