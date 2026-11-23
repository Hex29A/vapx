use anyhow::Context;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::debug;

use crate::config::cameras::load_cameras;

// No `Debug`: the struct holds the password, and a stray `{:?}` would print it.
#[derive(Clone)]
#[allow(dead_code)]
pub struct Credentials {
    pub user: String,
    pub pass: String,
    pub https: bool,
    pub verify_ssl: bool,
    pub port: u16,
    pub timeout: u64,
}

/// Set by the global `--https` flag: talk HTTPS (and WSS) to cameras that are
/// given on the command line with `-u/-p` instead of by name from cameras.yaml.
static FORCE_HTTPS: AtomicBool = AtomicBool::new(false);

pub fn set_force_https(on: bool) {
    FORCE_HTTPS.store(on, Ordering::Relaxed);
}

/// Credentials for a camera given on the command line (not in cameras.yaml).
fn flag_credentials(user: &str, pass: &str, https: bool, insecure: bool, port: Option<u16>) -> Credentials {
    Credentials {
        user: user.to_string(),
        pass: pass.to_string(),
        https,
        verify_ssl: !insecure,
        port: port.unwrap_or(if https { 443 } else { 80 }),
        timeout: 10,
    }
}

/// Resolve credentials and the effective host.
///
/// cameras.yaml is consulted first so that a configured camera *name* always
/// resolves to its real host, even when credentials are supplied on the command
/// line. Explicit -u/-p still win over the stored user and password — they
/// override the credentials, not the address.
///
/// Order:
/// 1. cameras.yaml lookup by name/host (with -u/-p as credential overrides)
/// 2. Explicit -u/-p flags for a host that is not in the config
/// 3. Interactive prompt (if TTY)
pub fn resolve(
    host: &str,
    user: Option<&str>,
    pass: Option<&str>,
    port: Option<u16>,
    insecure: bool,
) -> anyhow::Result<(Credentials, String)> {
    let force_https = FORCE_HTTPS.load(Ordering::Relaxed);

    // Try cameras.yaml first — the name must map to the configured host even
    // when the caller brings its own credentials.
    if let Some(config) = load_cameras()? {
        if let Some((name, entry)) = config.find(host) {
            debug!("Found camera '{}' in config (host: {})", host, entry.host);
            let effective_user = user
                .map(String::from)
                .or_else(|| config.effective_user(entry));
            let effective_pass = pass
                .map(String::from)
                .or_else(|| entry.pass.clone())
                .or_else(|| crate::cmd::config::keyring_lookup(name));

            if let (Some(u), Some(p)) = (effective_user, effective_pass) {
                let https = force_https || config.effective_https(entry);
                return Ok((
                    Credentials {
                        user: u,
                        pass: p,
                        https,
                        verify_ssl: if insecure { false } else { config.effective_verify_ssl(entry) },
                        port: port.or(entry.port).unwrap_or(if https { 443 } else { 80 }),
                        timeout: config.effective_timeout(entry),
                    },
                    entry.host.clone(),
                ));
            }
        }
    }

    // Not in the config — use the flags directly against the host as given.
    if let (Some(u), Some(p)) = (user, pass) {
        debug!("Using credentials from CLI flags");
        return Ok((flag_credentials(u, p, force_https, insecure, port), host.to_string()));
    }

    // Interactive prompt as fallback (only if TTY)
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("No credentials provided and stdin is not a terminal. Use -u/-p flags or cameras.yaml.");
    }

    let u = user
        .map(String::from)
        .unwrap_or_else(|| {
            eprint!("Username: ");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            input.trim().to_string()
        });
    let p = pass
        .map(String::from)
        .unwrap_or_else(|| {
            rpassword::prompt_password("Password: ")
                .context("Failed to read password")
                .unwrap()
        });

    Ok((flag_credentials(&u, &p, force_https, insecure, port), host.to_string()))
}

#[cfg(test)]
mod tests {
    use super::flag_credentials;

    #[test]
    fn a_camera_given_by_flags_is_plain_http_on_80_by_default() {
        let c = flag_credentials("root", "pass", false, false, None);
        assert!(!c.https);
        assert_eq!(c.port, 80);
        assert!(c.verify_ssl);
    }

    #[test]
    fn https_flag_means_tls_on_443_unless_a_port_is_given() {
        let c = flag_credentials("root", "pass", true, false, None);
        assert!(c.https);
        assert_eq!(c.port, 443);
        assert!(c.verify_ssl);
        assert_eq!(flag_credentials("root", "pass", true, false, Some(8443)).port, 8443);
    }

    #[test]
    fn insecure_switches_certificate_verification_off() {
        assert!(!flag_credentials("root", "pass", true, true, None).verify_ssl);
    }
}
