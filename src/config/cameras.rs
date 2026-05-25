use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::Context;
use serde::Deserialize;
use tracing::debug;

static ACTIVE_PROFILE: OnceLock<String> = OnceLock::new();

/// Set the active profile name (called from main.rs).
pub fn set_profile(name: String) {
    ACTIVE_PROFILE.set(name).ok();
}

/// Get the active profile name.
pub fn active_profile() -> Option<&'static str> {
    ACTIVE_PROFILE.get().map(|s| s.as_str())
}

#[derive(Debug, Deserialize, Clone)]
pub struct CamerasConfig {
    #[serde(default)]
    pub defaults: Option<CameraDefaults>,
    pub cameras: HashMap<String, CameraEntry>,
    #[serde(default)]
    pub groups: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub profiles: HashMap<String, CameraDefaults>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CameraDefaults {
    pub user: Option<String>,
    pub https: Option<bool>,
    pub verify_ssl: Option<bool>,
    pub timeout: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CameraEntry {
    pub host: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub pass: Option<String>,
    #[serde(default)]
    pub https: Option<bool>,
    #[serde(default)]
    pub verify_ssl: Option<bool>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub timeout: Option<u64>,
}

impl CamerasConfig {
    /// Find a camera by name or host
    pub fn find(&self, identifier: &str) -> Option<(&String, &CameraEntry)> {
        // Try by name first
        if let Some(entry) = self.cameras.get(identifier) {
            return Some((self.cameras.keys().find(|k| k.as_str() == identifier).unwrap(), entry));
        }
        // Try by host
        self.cameras
            .iter()
            .find(|(_, entry)| entry.host == identifier)
    }

    /// Get all cameras in a group
    #[allow(dead_code)]
    pub fn group(&self, name: &str) -> Vec<(&String, &CameraEntry)> {
        self.groups
            .get(name)
            .map(|names| {
                names
                    .iter()
                    .filter_map(|n| self.cameras.get(n).map(|e| (n, e)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get the effective user for a camera (camera override > profile > defaults)
    pub fn effective_user(&self, entry: &CameraEntry) -> Option<String> {
        entry
            .user
            .clone()
            .or_else(|| self.profile_defaults().and_then(|p| p.user.clone()))
            .or_else(|| self.defaults.as_ref().and_then(|d| d.user.clone()))
    }

    /// Get the effective https setting for a camera
    pub fn effective_https(&self, entry: &CameraEntry) -> bool {
        entry
            .https
            .or_else(|| self.profile_defaults().and_then(|p| p.https))
            .or_else(|| self.defaults.as_ref().and_then(|d| d.https))
            .unwrap_or(false)
    }

    /// Get the effective verify_ssl setting
    pub fn effective_verify_ssl(&self, entry: &CameraEntry) -> bool {
        entry
            .verify_ssl
            .or_else(|| self.profile_defaults().and_then(|p| p.verify_ssl))
            .or_else(|| self.defaults.as_ref().and_then(|d| d.verify_ssl))
            .unwrap_or(false)
    }

    /// Get the effective timeout
    pub fn effective_timeout(&self, entry: &CameraEntry) -> u64 {
        entry
            .timeout
            .or_else(|| self.profile_defaults().and_then(|p| p.timeout))
            .or_else(|| self.defaults.as_ref().and_then(|d| d.timeout))
            .unwrap_or(10)
    }

    /// Get the active profile's defaults (if any).
    fn profile_defaults(&self) -> Option<&CameraDefaults> {
        active_profile().and_then(|name| self.profiles.get(name))
    }
}

static CONFIG_OVERRIDE: OnceLock<PathBuf> = OnceLock::new();

/// Set an explicit config path (from --config flag).
pub fn set_config_path(path: PathBuf) {
    CONFIG_OVERRIDE.set(path).ok();
}

/// Resolve the config file path. Search order:
/// 1. --config flag (set via set_config_path)
/// 2. $VAPX_CONFIG env var
/// 3. ~/.config/vapx/cameras.yaml (XDG)
pub fn config_path() -> Option<PathBuf> {
    // Explicit --config flag
    if let Some(p) = CONFIG_OVERRIDE.get() {
        if p.exists() {
            return Some(p.clone());
        }
    }

    // Explicit env var
    if let Ok(path) = std::env::var("VAPX_CONFIG") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    // XDG config
    if let Some(config_dir) = dirs::config_dir() {
        let xdg = config_dir.join("vapx").join("cameras.yaml");
        if xdg.exists() {
            return Some(xdg);
        }
    }

    None
}

/// Load and parse the cameras config, substituting env vars in values.
pub fn load_cameras() -> anyhow::Result<Option<CamerasConfig>> {
    let path = match config_path() {
        Some(p) => p,
        None => return Ok(None),
    };

    debug!("Loading cameras config from: {}", path.display());
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    // Substitute ${ENV_VAR} patterns
    let content = substitute_env_vars(&content);

    let config: CamerasConfig =
        serde_yaml::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))?;

    Ok(Some(config))
}

/// Replace ${VAR_NAME} with environment variable values.
fn substitute_env_vars(input: &str) -> String {
    let mut result = input.to_string();
    let re_pattern = "${";

    while let Some(start) = result.find(re_pattern) {
        let after_start = start + 2;
        if let Some(end) = result[after_start..].find('}') {
            let var_name = &result[after_start..after_start + end];
            let replacement = std::env::var(var_name).unwrap_or_default();
            result = format!(
                "{}{}{}",
                &result[..start],
                replacement,
                &result[after_start + end + 1..]
            );
        } else {
            break;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_env_vars() {
        std::env::set_var("TEST_VAPX_PASS", "secret123");
        let input = "pass: \"${TEST_VAPX_PASS}\"";
        let result = substitute_env_vars(input);
        assert_eq!(result, "pass: \"secret123\"");
        std::env::remove_var("TEST_VAPX_PASS");
    }

    #[test]
    fn test_substitute_missing_var() {
        let input = "pass: \"${NONEXISTENT_VAR_XYZ}\"";
        let result = substitute_env_vars(input);
        assert_eq!(result, "pass: \"\"");
    }

    #[test]
    fn test_parse_config() {
        let yaml = r#"
defaults:
  user: root
  https: false
  verify_ssl: false
  timeout: 10

cameras:
  testcam:
    host: 192.168.1.100
    pass: "secret"
  othercam:
    host: 192.168.1.101
    user: admin
    pass: "other"
    https: true

groups:
  all:
    - testcam
    - othercam
"#;
        let config: CamerasConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.cameras.len(), 2);
        assert_eq!(config.cameras["testcam"].host, "192.168.1.100");
        assert_eq!(config.effective_user(&config.cameras["testcam"]), Some("root".into()));
        assert_eq!(config.effective_user(&config.cameras["othercam"]), Some("admin".into()));
        assert!(!config.effective_https(&config.cameras["testcam"]));
        assert!(config.effective_https(&config.cameras["othercam"]));
        assert_eq!(config.group("all").len(), 2);
    }

    #[test]
    fn test_find_by_name_and_host() {
        let yaml = r#"
cameras:
  mycam:
    host: 10.0.0.5
    pass: "pw"
"#;
        let config: CamerasConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.find("mycam").is_some());
        assert!(config.find("10.0.0.5").is_some());
        assert!(config.find("unknown").is_none());
    }
}
