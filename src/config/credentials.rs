use anyhow::Context;
use tracing::debug;

use crate::config::cameras::load_cameras;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Credentials {
    pub user: String,
    pub pass: String,
    pub https: bool,
    pub verify_ssl: bool,
    pub port: u16,
    pub timeout: u64,
}

/// Resolve credentials in priority order:
/// 1. Explicit -u/-p flags
/// 2. cameras.yaml lookup by host/name
/// 3. Interactive prompt (if TTY)
pub fn resolve(
    host: &str,
    user: Option<&str>,
    pass: Option<&str>,
    port: Option<u16>,
    insecure: bool,
) -> anyhow::Result<(Credentials, String)> {
    // If both user and pass are given, use them directly
    if let (Some(u), Some(p)) = (user, pass) {
        debug!("Using credentials from CLI flags");
        return Ok((
            Credentials {
                user: u.to_string(),
                pass: p.to_string(),
                https: false,
                verify_ssl: !insecure,
                port: port.unwrap_or(80),
                timeout: 10,
            },
            host.to_string(),
        ));
    }

    // Try cameras.yaml
    if let Some(config) = load_cameras()? {
        if let Some((_name, entry)) = config.find(host) {
            debug!("Found camera '{}' in config (host: {})", host, entry.host);
            let effective_user = user
                .map(String::from)
                .or_else(|| config.effective_user(entry));
            let effective_pass = pass
                .map(String::from)
                .or_else(|| entry.pass.clone());

            if let (Some(u), Some(p)) = (effective_user, effective_pass) {
                return Ok((
                    Credentials {
                        user: u,
                        pass: p,
                        https: config.effective_https(entry),
                        verify_ssl: if insecure { false } else { config.effective_verify_ssl(entry) },
                        port: port.or(entry.port).unwrap_or(if config.effective_https(entry) { 443 } else { 80 }),
                        timeout: config.effective_timeout(entry),
                    },
                    entry.host.clone(),
                ));
            }
        }
    }

    // Interactive prompt as fallback (only if TTY)
    if !atty::is(atty::Stream::Stdin) {
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

    Ok((
        Credentials {
            user: u,
            pass: p,
            https: false,
            verify_ssl: !insecure,
            port: port.unwrap_or(80),
            timeout: 10,
        },
        host.to_string(),
    ))
}
