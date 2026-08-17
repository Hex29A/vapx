use std::io::{IsTerminal, Write};

use clap::{Args, Subcommand};

use crate::config::cameras;
use crate::config::credentials;
use crate::config::writer;
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
    /// Rename a camera, updating its group memberships too
    Rename {
        /// Current name in cameras.yaml
        from: String,
        /// New name
        to: String,
    },
    /// Remove a camera: its entry, group memberships, comments and keyring secret
    #[command(visible_alias = "delete", visible_alias = "rm")]
    Remove {
        /// Camera name (as defined in cameras.yaml)
        name: String,
        /// Do not ask for confirmation
        #[arg(short = 'y', long)]
        yes: bool,
        /// Leave the password in the OS keyring
        #[arg(long)]
        keep_secret: bool,
    },
    /// Manage camera groups (targets for `vapx batch`)
    Group {
        #[command(subcommand)]
        command: GroupCommands,
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

#[derive(Subcommand)]
pub enum GroupCommands {
    /// List groups and their members
    List,
    /// Add a camera to an existing group
    Add {
        /// Group name
        group: String,
        /// Camera name (as defined in cameras.yaml)
        camera: String,
    },
    /// Remove a camera from a group
    Remove {
        /// Group name
        group: String,
        /// Camera name
        camera: String,
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

                let config_path = cameras::config_path()
                    .or_else(|| dirs::config_dir().map(|d| d.join("vapx").join("cameras.yaml")))
                    .unwrap_or_else(|| std::path::PathBuf::from("cameras.yaml"));

                // A camera without a password is unusable, and the vapx-mcp
                // server rejects the whole config over one such entry.
                if pass.is_none() {
                    eprintln!(
                        "Warning: no password set for '{}'. Add `pass:` to the entry, or store one with `vapx config set-secret {}`.",
                        name, name
                    );
                }

                // Seed a fresh file with the commented template so the result
                // is a config a human wants to keep editing.
                if !config_path.exists() {
                    if let Some(parent) = config_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&config_path, TEMPLATE_CONFIG)?;
                }

                writer::add_camera(
                    &config_path,
                    &writer::NewCamera {
                        name: name.clone(),
                        host: host.clone(),
                        user,
                        pass,
                        https,
                        port,
                    },
                )?;

                format::ok_msg(&format!("Added camera '{}' ({}) to {}", name, host, config_path.display()));
            }
            ConfigCommands::Rename { from, to } => {
                let config_path = cameras::config_path()
                    .ok_or_else(|| anyhow::anyhow!("No config file found"))?;
                writer::rename_camera(&config_path, &from, &to)?;
                format::ok_msg(&format!("Renamed '{}' to '{}' in {}", from, to, config_path.display()));
            }
            ConfigCommands::Remove {
                name,
                yes,
                keep_secret,
            } => {
                let config_path = cameras::config_path()
                    .ok_or_else(|| anyhow::anyhow!("No config file found"))?;
                let config = cameras::load_cameras()?
                    .ok_or_else(|| anyhow::anyhow!("No config file found"))?;
                let entry = config.cameras.get(&name).ok_or_else(|| {
                    anyhow::anyhow!("Camera '{}' not found in {}", name, config_path.display())
                })?;

                // Removing is destructive, so say what goes before it goes.
                let mut in_groups: Vec<&str> = config
                    .groups
                    .iter()
                    .filter(|(_, members)| members.iter().any(|m| m == &name))
                    .map(|(g, _)| g.as_str())
                    .collect();
                in_groups.sort();

                if !yes {
                    if !std::io::stdin().is_terminal() {
                        anyhow::bail!(
                            "Refusing to remove '{}' without confirmation: pass -y when running non-interactively",
                            name
                        );
                    }
                    eprintln!("Remove '{}' ({}) from {}?", name, entry.host, config_path.display());
                    if !in_groups.is_empty() {
                        eprintln!("  also leaves group(s): {}", in_groups.join(", "));
                    }
                    if !keep_secret {
                        eprintln!("  also deletes its keyring entry, if any");
                    }
                    eprint!("Type the camera name to confirm: ");
                    std::io::stderr().flush().ok();
                    let mut answer = String::new();
                    std::io::stdin().read_line(&mut answer)?;
                    if answer.trim() != name {
                        // err_json exits with status 1 — nothing has been written yet.
                        format::err_json("CANCELLED", "Name did not match — nothing was removed");
                    }
                }

                let removed = writer::remove_camera(&config_path, &name)?;

                // Best effort: a missing entry (or a build without keyring
                // support) is not a reason to fail a removal that succeeded.
                let secret = if keep_secret {
                    "kept"
                } else if forget_keyring_secret(&name) {
                    "removed"
                } else {
                    "none"
                };

                format::ok(&serde_json::json!({
                    "removed": name,
                    "host": entry.host,
                    "groups": removed.groups,
                    "comment_lines": removed.comment_lines,
                    "keyring": secret,
                    "config": config_path.display().to_string(),
                }));
            }
            ConfigCommands::Group { command } => {
                let config_path = cameras::config_path()
                    .ok_or_else(|| anyhow::anyhow!("No config file found"))?;
                match command {
                    GroupCommands::List => {
                        let config = cameras::load_cameras()?
                            .ok_or_else(|| anyhow::anyhow!("No config file found"))?;
                        let mut names: Vec<&String> = config.groups.keys().collect();
                        names.sort();
                        let out: Vec<serde_json::Value> = names
                            .iter()
                            .map(|g| serde_json::json!({
                                "group": g,
                                "members": config.groups[*g],
                            }))
                            .collect();
                        format::ok(&out);
                    }
                    GroupCommands::Add { group, camera } => {
                        // Fail before writing if the camera is not configured:
                        // a group of names that resolve to nothing is worse
                        // than an error here.
                        let config = cameras::load_cameras()?
                            .ok_or_else(|| anyhow::anyhow!("No config file found"))?;
                        if !config.cameras.contains_key(&camera) {
                            anyhow::bail!(
                                "Camera '{}' is not in cameras.yaml — add it first with `vapx config add` or `vapx enroll`",
                                camera
                            );
                        }
                        writer::add_to_group(&config_path, &group, &camera)?;
                        format::ok_msg(&format!("'{}' is now in group '{}'", camera, group));
                    }
                    GroupCommands::Remove { group, camera } => {
                        writer::remove_from_group(&config_path, &group, &camera)?;
                        format::ok_msg(&format!("'{}' is no longer in group '{}'", camera, group));
                    }
                }
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
  verify_ssl: true     # TLS cert verification (only applies when https: true)
  timeout: 10          # seconds; increase for WAN cameras (e.g. 30)

cameras:
  # my-camera:
  #   host: 192.168.1.100
  #   pass: "${MY_CAMERA_PASS}"   # or plain text (not recommended)
  #   user: root                  # overrides defaults.user
  #   https: false
  #   verify_ssl: true
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

/// Delete a camera's keyring entry without failing the caller.
///
/// Used by `config remove`, where the camera is going away regardless: no entry
/// (or a build without keyring support) means there was nothing to clean up,
/// which is a fine outcome rather than an error.
#[cfg(feature = "keyring")]
fn forget_keyring_secret(name: &str) -> bool {
    keyring::Entry::new(KEYRING_SERVICE, name)
        .map(|e| e.delete_credential().is_ok())
        .unwrap_or(false)
}

#[cfg(not(feature = "keyring"))]
fn forget_keyring_secret(_name: &str) -> bool {
    false
}
