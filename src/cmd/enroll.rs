use clap::{Args, ValueEnum};

use crate::config::cameras;
use crate::config::credentials::Credentials;
use crate::config::writer;
use crate::enroll::{naming, password};
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::{device, systemready, users};

#[derive(Clone, Copy, ValueEnum, PartialEq)]
pub enum EnrollRole {
    Admin,
    Operator,
    Viewer,
}

impl EnrollRole {
    fn sgrp(&self) -> &'static str {
        // PTZ is always included: the initial account is documented as
        // requiring Administrator *with* PTZ, and modern firmware adds the
        // group by itself anyway.
        match self {
            EnrollRole::Admin => "admin:operator:viewer:ptz",
            EnrollRole::Operator => "operator:viewer:ptz",
            EnrollRole::Viewer => "viewer:ptz",
        }
    }
}

#[derive(Args)]
pub struct EnrollCmd {
    /// Camera IP or hostname (a factory-default camera is not in cameras.yaml yet)
    pub host: String,

    /// Name for the camera in cameras.yaml. Derived from model and serial if omitted.
    #[arg(long)]
    pub name: Option<String>,

    /// Account name to create on the camera.
    ///
    /// Defaults to root, which is the only name accepted by AXIS OS older
    /// than 11.5 — and the version cannot be read before the account exists.
    #[arg(long, default_value = "root")]
    pub account: String,

    /// Role for the created account
    #[arg(long, default_value = "admin")]
    pub role: EnrollRole,

    /// Add the camera to this group in cameras.yaml. Left out of every group if omitted.
    #[arg(long)]
    pub to_group: Option<String>,

    /// Use this password instead of generating one (validated against the device policy)
    #[arg(long)]
    pub pwd: Option<String>,

    /// Length of the generated password
    #[arg(long)]
    pub pwd_length: Option<usize>,

    /// Print the password in clear text instead of masking it
    #[arg(long)]
    pub reveal: bool,

    /// Also write the full result, including the password, to this file (mode 0600)
    #[arg(long)]
    pub out: Option<std::path::PathBuf>,

    /// Show what would happen without touching the camera or the config
    #[arg(long)]
    pub dry_run: bool,

    /// Enroll even if the camera already has an account (creates an extra account)
    #[arg(long)]
    pub force: bool,

    #[arg(long)]
    pub port: Option<u16>,

    /// Request timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,

    /// Seconds to wait for the camera to become ready
    #[arg(long, default_value = "60")]
    pub wait: u64,
}

impl EnrollCmd {
    pub fn run(self) -> anyhow::Result<()> {
        // 1. Probe without credentials — the only thing that works on a
        //    factory-default camera.
        let anon = self.anonymous_client();
        // Name the host but do not assert *why* it failed — the error may
        // already say the endpoint is missing rather than unreachable.
        let state = systemready::query(&anon, 10)
            .map_err(|e| anyhow::anyhow!("{}: {}", self.host, e))?;

        if !state.systemready {
            anyhow::bail!(
                "Camera is still booting (systemready=no). Retry, or use `vapx systemready {} --until-ready {}`.",
                self.host,
                self.wait
            );
        }

        if !state.needsetup && !self.force {
            anyhow::bail!(
                "Camera already has an account (needsetup=no). Use --force to add another account, \
                 or `vapx config add` if it only needs a config entry."
            );
        }

        let policy = state.passphrase_policy;

        // 2. Password: generate to the device's stated policy, or validate the
        //    one we were handed against it before the camera rejects it.
        let pwd = match self.pwd.clone() {
            Some(p) => {
                password::satisfies(&p, policy)
                    .map_err(|e| anyhow::anyhow!("Supplied password rejected: {}", e))?;
                p
            }
            None => password::generate(policy, self.pwd_length),
        };

        let existing_names: Vec<String> = cameras::load_cameras()
            .ok()
            .flatten()
            .map(|c| c.cameras.keys().cloned().collect())
            .unwrap_or_default();

        if let Some(ref n) = self.name {
            if existing_names.iter().any(|e| e == n) {
                anyhow::bail!("Camera '{}' already exists in cameras.yaml", n);
            }
        }

        // Check the group before touching the camera. Discovering a typo after
        // the account exists would leave the camera enrolled but unfiled, and
        // the account cannot be created twice.
        if let Some(ref g) = self.to_group {
            let groups = cameras::load_cameras()
                .ok()
                .flatten()
                .map(|c| {
                    let mut g: Vec<String> = c.groups.keys().cloned().collect();
                    g.sort();
                    g
                })
                .unwrap_or_default();
            if !groups.iter().any(|x| x == g) {
                anyhow::bail!(
                    "Group '{}' does not exist in cameras.yaml. Existing groups: {}",
                    g,
                    if groups.is_empty() { "(none)".into() } else { groups.join(", ") }
                );
            }
        }

        if self.dry_run {
            format::ok(&serde_json::json!({
                "dry_run": true,
                "host": self.host,
                "needsetup": state.needsetup,
                "passphrase_policy": policy.as_str(),
                "account": self.account,
                "role": self.role.sgrp(),
                "password_length": pwd.chars().count(),
                "name": self.name.clone(),
                "name_note": if self.name.is_none() {
                    "derived from model and serial after the account is created — not knowable yet"
                } else { "given on the command line" },
                "group": self.to_group.clone(),
                "would_write": cameras::config_path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(no config file found)".into()),
            }));
            return Ok(());
        }

        // 3. Create the account. Unauthenticated: this is the one window in
        //    which that is possible, and it closes as soon as it succeeds.
        users::add(
            &anon,
            &self.account,
            &pwd,
            self.role.sgrp(),
            "",
            if state.needsetup {
                users::PrimaryGroup::Root
            } else {
                users::PrimaryGroup::Users
            },
            true,
        )
        .map_err(|e| anyhow::anyhow!("Could not create account '{}': {}", self.account, e))?;

        // 4. Verify the account actually works before anything is written.
        let creds = Credentials {
            user: self.account.clone(),
            pass: pwd.clone(),
            https: false,
            verify_ssl: false,
            port: self.port.unwrap_or(80),
            timeout: self.timeout.unwrap_or(10),
        };
        let client = crate::cmd::make_client(&self.host, creds, self.timeout);

        let props = device::get_all_properties(&client).map_err(|e| {
            anyhow::anyhow!(
                "Account '{}' was created but logging in with it failed: {}. \
                 The password is: {} — save it, the camera now requires it.",
                self.account,
                e,
                pwd
            )
        })?;

        let get = |k: &str| {
            props
                .pointer(&format!("/data/propertyList/{}", k))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        let model = get("ProdNbr");
        let serial = get("SerialNumber");
        let version = get("Version");

        // 5. Only now can the name be derived — model and serial are behind
        //    authentication, which did not exist a moment ago.
        let name = match self.name.clone() {
            Some(n) => n,
            None => naming::derive_unique(&model, &serial, &existing_names),
        };

        // 6. Write the config entry. If this fails the credentials are still
        //    reported, so the camera is never left with a lost password.
        let config_path = cameras::config_path()
            .or_else(|| dirs::config_dir().map(|d| d.join("vapx").join("cameras.yaml")))
            .unwrap_or_else(|| std::path::PathBuf::from("cameras.yaml"));

        let write_result = writer::add_camera(
            &config_path,
            &writer::NewCamera {
                name: name.clone(),
                host: self.host.clone(),
                user: Some(self.account.clone()),
                pass: Some(pwd.clone()),
                https: false,
                port: self.port,
            },
        );

        let group_result = match (&self.to_group, &write_result) {
            (Some(g), Ok(())) => match writer::add_to_group(&config_path, g, &name) {
                Ok(()) => Some(format!("added to group '{}'", g)),
                Err(e) => Some(format!("group '{}' not updated: {}", g, e)),
            },
            (Some(_), Err(_)) => Some("group not updated (config write failed)".into()),
            (None, _) => None,
        };

        let mut result = serde_json::json!({
            "name": name,
            "host": self.host,
            "model": model,
            "serial": serial,
            "firmware": version,
            "account": self.account,
            "role": self.role.sgrp(),
            "passphrase_policy": policy.as_str(),
            "password": if self.reveal { pwd.clone() } else { mask(&pwd) },
            "config": match &write_result {
                Ok(()) => format!("written to {}", config_path.display()),
                Err(e) => format!("NOT WRITTEN: {}", e),
            },
        });
        if let Some(g) = group_result {
            result["group"] = serde_json::Value::String(g);
        }

        // The credentials file, if asked for, always carries the real password
        // — that is its whole purpose — and is owner-only.
        if let Some(ref out) = self.out {
            let mut full = result.clone();
            full["password"] = serde_json::Value::String(pwd.clone());
            write_secret_file(out, &serde_json::to_string_pretty(&full)?)?;
            result["out"] = serde_json::Value::String(out.display().to_string());
        }

        if !self.reveal && self.out.is_none() {
            result["password_note"] = serde_json::Value::String(
                "masked — rerun with --reveal or --out <file> to obtain it".into(),
            );
        }

        if let Err(e) = &write_result {
            format::err_json(
                "CONFIG_WRITE_FAILED",
                &format!(
                    "Account created on {} but the config was not updated: {}. Credentials: {} / {}",
                    self.host, e, self.account, pwd
                ),
            );
        }

        format::ok(&result);
        Ok(())
    }

    /// A client with no credentials, for the pre-account endpoints.
    fn anonymous_client(&self) -> VapixClient {
        let creds = Credentials {
            user: String::new(),
            pass: String::new(),
            https: false,
            verify_ssl: false,
            port: self.port.unwrap_or(80),
            timeout: self.timeout.unwrap_or(10),
        };
        crate::cmd::make_client(&self.host, creds, self.timeout)
    }
}

fn mask(pw: &str) -> String {
    let n = pw.chars().count();
    format!("{}… ({} chars)", pw.chars().take(2).collect::<String>(), n)
}

fn write_secret_file(path: &std::path::Path, content: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    let mut f = std::fs::File::create(path)?;
    let mut perms = f.metadata()?.permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(path, perms)?;
    f.write_all(content.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_role_always_includes_ptz() {
        // The initial account is documented as requiring Administrator with
        // PTZ on every AXIS OS generation.
        assert_eq!(EnrollRole::Admin.sgrp(), "admin:operator:viewer:ptz");
        assert!(EnrollRole::Operator.sgrp().contains("ptz"));
        assert!(EnrollRole::Viewer.sgrp().contains("ptz"));
    }

    #[test]
    fn mask_hides_the_password() {
        let m = mask("Abcdefghij123456789-");
        assert!(m.starts_with("Ab"));
        assert!(m.contains("20 chars"));
        assert!(!m.contains("cdefghij"));
    }
}
