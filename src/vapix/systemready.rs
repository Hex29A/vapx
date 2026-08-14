//! Systemready API — the one VAPIX endpoint that answers without credentials.
//!
//! This is what makes enrolling a factory-default camera possible: `needsetup`
//! reports whether the device still lacks its initial administrator account,
//! and `passphrasepolicy` states the password rules to satisfy — both before
//! any account exists to authenticate with.
//!
//! Ref: https://developer.axis.com/vapix/network-video/systemready-api/

use serde_json::Value;

use crate::vapix::client::VapixClient;

/// Ask for a recent version; the device negotiates down to what it supports.
/// Measured: AXIS OS 12 answers 1.5, older devices answer 1.2 and omit
/// `uptime`, `bootid` and `passphrasepolicy` entirely.
const API_VERSION: &str = "1.4";

/// The device's passphrase policy, as reported by systemready.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PassphrasePolicy {
    /// No policy configured.
    None,
    /// Length based: minimum 15 characters.
    Length,
    /// Complexity based: minimum 12 characters, and at least one each of
    /// special, digit, uppercase and lowercase.
    Complex,
    /// Field absent — older AXIS OS. Treat as the strictest policy.
    Unknown,
}

impl PassphrasePolicy {
    pub fn from_str(s: Option<&str>) -> Self {
        match s {
            Some("none") => PassphrasePolicy::None,
            Some("length") => PassphrasePolicy::Length,
            Some("complex") => PassphrasePolicy::Complex,
            _ => PassphrasePolicy::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PassphrasePolicy::None => "none",
            PassphrasePolicy::Length => "length",
            PassphrasePolicy::Complex => "complex",
            PassphrasePolicy::Unknown => "unknown",
        }
    }
}

/// Parsed systemready response.
#[derive(Debug, Clone)]
pub struct SystemReady {
    pub systemready: bool,
    /// True when the device has no initial administrator account yet.
    pub needsetup: bool,
    pub passphrase_policy: PassphrasePolicy,
    pub uptime: Option<u64>,
    pub preview_mode: Option<String>,
    /// Identifies the current boot; changes on every restart.
    pub bootid: Option<String>,
    /// The raw data object, for callers that want to print everything.
    pub raw: Value,
}

impl SystemReady {
    fn from_data(data: &Value) -> Self {
        let yes = |k: &str| data.get(k).and_then(|v| v.as_str()) == Some("yes");
        SystemReady {
            systemready: yes("systemready"),
            needsetup: yes("needsetup"),
            passphrase_policy: PassphrasePolicy::from_str(
                data.get("passphrasepolicy").and_then(|v| v.as_str()),
            ),
            uptime: data
                .get("uptime")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok()),
            preview_mode: data
                .get("previewmode")
                .and_then(|v| v.as_str())
                .map(String::from),
            bootid: data
                .get("bootid")
                .and_then(|v| v.as_str())
                .map(String::from),
            raw: data.clone(),
        }
    }
}

/// Query systemready. `wait` is how many seconds the device may hold the
/// request open while it finishes booting (it answers as soon as it is ready).
pub fn query(client: &VapixClient, wait: u64) -> anyhow::Result<SystemReady> {
    let body = serde_json::json!({
        "apiVersion": API_VERSION,
        "context": "vapx",
        "method": "systemready",
        "params": { "timeout": wait },
    });
    let resp = client.post_json("/axis-cgi/systemready.cgi", &body)?;
    let data = resp.get("data").unwrap_or(&resp);
    Ok(SystemReady::from_data(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_factory_default_response() {
        // Captured verbatim from an M1137 Mk II (AXIS OS 12.11.77) right after
        // a soft factory default.
        let data = serde_json::json!({
            "systemready": "yes",
            "needsetup": "yes",
            "uptime": "117",
            "bootid": "1e566f78-4816-4996-9bef-ec3271273acc",
            "previewmode": "7200",
            "passphrasepolicy": "none"
        });
        let s = SystemReady::from_data(&data);
        assert!(s.systemready);
        assert!(s.needsetup);
        assert_eq!(s.uptime, Some(117));
        assert_eq!(s.preview_mode.as_deref(), Some("7200"));
        assert_eq!(
            s.bootid.as_deref(),
            Some("1e566f78-4816-4996-9bef-ec3271273acc")
        );
        assert_eq!(s.passphrase_policy, PassphrasePolicy::None);
    }

    #[test]
    fn parses_configured_camera_response() {
        let data = serde_json::json!({
            "systemready": "yes",
            "needsetup": "no",
            "uptime": "3094407",
            "passphrasepolicy": "length"
        });
        let s = SystemReady::from_data(&data);
        assert!(!s.needsetup);
        assert_eq!(s.passphrase_policy, PassphrasePolicy::Length);
    }

    #[test]
    fn older_firmware_omits_policy_and_is_treated_as_unknown() {
        // Measured on a C Cube LW / M3045-V: apiVersion 1.2, only two fields.
        let data = serde_json::json!({"systemready": "yes", "needsetup": "no"});
        let s = SystemReady::from_data(&data);
        assert!(s.systemready);
        assert!(!s.needsetup);
        assert_eq!(s.passphrase_policy, PassphrasePolicy::Unknown);
        assert_eq!(s.uptime, None);
        assert_eq!(s.bootid, None, "older firmware omits bootid");
    }

    #[test]
    fn still_booting() {
        let data = serde_json::json!({"systemready": "no", "needsetup": "yes"});
        let s = SystemReady::from_data(&data);
        assert!(!s.systemready);
        assert!(s.needsetup);
    }

    #[test]
    fn policy_round_trip() {
        assert_eq!(PassphrasePolicy::from_str(Some("complex")), PassphrasePolicy::Complex);
        assert_eq!(PassphrasePolicy::from_str(None), PassphrasePolicy::Unknown);
        assert_eq!(PassphrasePolicy::Complex.as_str(), "complex");
    }
}
