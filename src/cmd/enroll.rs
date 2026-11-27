use clap::{Args, ValueEnum};

use crate::cmd::pwd::PwdArgs;
use crate::config::cameras;
use crate::config::credentials::Credentials;
use crate::config::{envfile, writer};
use crate::enroll::{naming, password};
use crate::output::format;
use crate::vapix::client::VapixClient;
use crate::vapix::{device, selftest, systemready, users};

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

    /// Use a password of your own instead of a generated one (validated against the device
    /// policy): --pwd-env or --pwd-file keep it out of the process list
    #[command(flatten)]
    pub pwd: PwdArgs,

    /// Length of the generated password
    #[arg(long)]
    pub pwd_length: Option<usize>,

    /// Print the password in clear text instead of masking it
    #[arg(long)]
    pub reveal: bool,

    /// Also write the full result, including the password, to this file (mode 0600)
    #[arg(long)]
    pub out: Option<std::path::PathBuf>,

    /// Keep the password in an env file and write `pass: "${NAME}"` to cameras.yaml.
    /// This is the default, as VAPX_PASS_<CAMERA NAME>, when the env file already exists
    #[arg(long, value_name = "NAME", conflicts_with = "pass_inline")]
    pub pass_env: Option<String>,

    /// The env file for the password (default: `env` next to cameras.yaml). Created with
    /// mode 0600; the line is appended and nothing in it is rewritten
    #[arg(long, value_name = "PATH", conflicts_with = "pass_inline")]
    pub env_file: Option<std::path::PathBuf>,

    /// Write the password in clear text to cameras.yaml even when an env file exists
    #[arg(long)]
    pub pass_inline: bool,

    /// Run the device self-test before the account is created. It only works then,
    /// while the camera is still in its initial setup (AXIS OS 12 and later)
    #[arg(long)]
    pub selftest: bool,

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
        let pwd = match self.pwd.given()? {
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

        // Where the password goes, decided and checked before the camera is
        // touched: an env name that is taken or a password the env file cannot
        // hold must not be found after the account exists.
        let config_path = cameras::config_write_path();
        let env_plan = self.env_plan(&config_path, &pwd)?;

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
                "would_write": config_path.display().to_string(),
                "password_storage": match &env_plan {
                    Some(plan) => format!(
                        "env file {} as {}",
                        plan.path.display(),
                        plan.name.as_deref().unwrap_or("VAPX_PASS_<CAMERA NAME>")
                    ),
                    None => "cameras.yaml (clear text)".to_string(),
                },
                "selftest": if self.selftest { "would run before the account is created" } else { "not requested" },
            }));
            return Ok(());
        }

        // The self-test is only open before the account exists (afterwards
        // the camera answers 2105 "not in preview mode"). Its result is
        // reported; it does not stop the enrollment.
        let selftest_result = if self.selftest {
            let client = crate::cmd::make_client(
                &self.host,
                self.anonymous_creds(),
                Some(self.timeout.unwrap_or(10).max(120)),
            );
            Some(match selftest::run_self_test(&client) {
                Ok(resp) => resp.get("data").cloned().unwrap_or(resp),
                Err(e) => serde_json::json!({ "error": e.to_string() }),
            })
        } else {
            None
        };

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
                "Account '{}' was created but logging in with it failed: {}. {}",
                self.account,
                e,
                self.keep_password(&pwd, false)
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
        //    With an env file the password goes there first, so cameras.yaml
        //    never refers to a variable that holds nothing.
        let write_result = store_in_env(env_plan.as_ref(), &name, &pwd).and_then(|var| {
            writer::add_camera(
                &config_path,
                &writer::NewCamera {
                    name: name.clone(),
                    host: self.host.clone(),
                    user: Some(self.account.clone()),
                    pass: Some(match &var {
                        Some(v) => format!("${{{}}}", v),
                        None => pwd.clone(),
                    }),
                    https: false,
                    port: self.port,
                },
            )?;
            Ok(var)
        });

        let group_result = match (&self.to_group, &write_result) {
            (Some(g), Ok(_)) => match writer::add_to_group(&config_path, g, &name) {
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
                Ok(_) => format!("written to {}", config_path.display()),
                Err(e) => format!("NOT WRITTEN: {}", e),
            },
        });
        if let (Ok(Some(var)), Some(plan)) = (&write_result, &env_plan) {
            result["password_stored"] = serde_json::Value::String(format!(
                "{} in {}; cameras.yaml refers to ${{{}}}, so load that file into the environment before running vapx",
                var,
                plan.path.display(),
                var
            ));
        }
        if let Some(st) = selftest_result {
            result["selftest"] = st;
        }
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
                    "Account created on {} but the config was not updated: {}. {}",
                    self.host,
                    e,
                    self.keep_password(&pwd, self.out.is_some())
                ),
            );
        }

        format::ok(&result);
        Ok(())
    }

    /// Save the password of the account that now exists, so a failure further on does not lose
    /// it, and say where it is. The camera requires this password from now on. It goes to the
    /// `--out` file when there is one, else to a private file in the config directory. The
    /// password is printed only when no file could be written.
    fn keep_password(&self, pwd: &str, out_written: bool) -> String {
        let path = self.out.clone().unwrap_or_else(|| recovery_path(&self.host));
        if out_written {
            // The `--out` file already holds the full result with the password.
            return format!(
                "The password for '{}' is saved in {} (readable only by you). Keep it, the camera now requires it.",
                self.account,
                path.display()
            );
        }
        let content = serde_json::json!({"host": self.host, "account": self.account, "password": pwd});
        let saved = path
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
            .map_or(Ok(()), std::fs::create_dir_all)
            .map_err(anyhow::Error::from)
            .and_then(|_| write_secret_file(&path, &serde_json::to_string_pretty(&content).unwrap_or_default()));
        match saved {
            Ok(()) => format!(
                "The password for '{}' is saved in {} (readable only by you). Keep it, the camera now requires it.",
                self.account,
                path.display()
            ),
            Err(e) => format!(
                "The password could not be saved ({e}), so here it is: {pwd}. Keep it, the camera now requires it."
            ),
        }
    }

    /// Where the password will be kept: an env file when `--pass-env` or
    /// `--env-file` asks for it or the default env file exists, else
    /// cameras.yaml. Fails before anything is created if the name is taken or
    /// the password cannot be written to an env file unchanged.
    fn env_plan(&self, config_path: &std::path::Path, pwd: &str) -> anyhow::Result<Option<EnvPlan>> {
        if self.pass_inline {
            return Ok(None);
        }
        let path = self.env_file.clone().unwrap_or_else(|| envfile::default_path(config_path));
        if self.pass_env.is_none() && self.env_file.is_none() && !path.exists() {
            return Ok(None);
        }
        if !envfile::value_is_safe(pwd) {
            anyhow::bail!(
                "The password cannot go into the env file {} unchanged: use only A-Z a-z 0-9 and -_.+=:@%#,/ \
                 (generated passwords always do), or --pass-inline to keep it in cameras.yaml",
                path.display()
            );
        }
        if let Some(ref name) = self.pass_env {
            envfile::validate_name(name)?;
            if envfile::names(&path)?.iter().any(|n| n == name) {
                anyhow::bail!("{} is already defined in {}; choose another --pass-env name", name, path.display());
            }
        } else {
            // Read it now, so an unreadable file stops us before the camera does.
            envfile::names(&path)?;
        }
        Ok(Some(EnvPlan { path, name: self.pass_env.clone() }))
    }

    /// A client with no credentials, for the pre-account endpoints.
    fn anonymous_client(&self) -> VapixClient {
        crate::cmd::make_client(&self.host, self.anonymous_creds(), self.timeout)
    }

    fn anonymous_creds(&self) -> Credentials {
        Credentials {
            user: String::new(),
            pass: String::new(),
            https: false,
            verify_ssl: false,
            port: self.port.unwrap_or(80),
            timeout: self.timeout.unwrap_or(10),
        }
    }
}

/// An env file the password goes into, and the variable name when it was
/// given (otherwise it is derived from the camera name).
struct EnvPlan {
    path: std::path::PathBuf,
    name: Option<String>,
}

/// Append the password to the env file, if there is one, and return the
/// variable that holds it.
fn store_in_env(plan: Option<&EnvPlan>, camera: &str, pwd: &str) -> anyhow::Result<Option<String>> {
    let Some(plan) = plan else { return Ok(None) };
    let var = match &plan.name {
        Some(n) => n.clone(),
        None => envfile::derive_name(camera, &envfile::names(&plan.path)?),
    };
    envfile::append(&plan.path, &var, pwd)
        .map_err(|e| anyhow::anyhow!("password not stored in {}: {}", plan.path.display(), e))?;
    Ok(Some(var))
}

fn mask(pw: &str) -> String {
    let n = pw.chars().count();
    format!("{}… ({} chars)", pw.chars().take(2).collect::<String>(), n)
}

/// Created with mode 0600 from the start: the file is never readable by others, not even briefly.
fn write_secret_file(path: &std::path::Path, content: &str) -> anyhow::Result<()> {
    crate::fsutil::write_private(path, content.as_bytes())
}

/// Where the password of a new account is kept when it would otherwise be lost.
fn recovery_path(host: &str) -> std::path::PathBuf {
    let safe: String = host.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-') { c } else { '_' }).collect();
    dirs::config_dir()
        .map(|d| d.join("vapx"))
        .unwrap_or_default()
        .join(format!("enroll-{safe}.recovery.json"))
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
