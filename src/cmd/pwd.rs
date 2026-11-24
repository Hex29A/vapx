//! Where the password of an account comes from. A value on the command line shows in the
//! process list and in the shell history, so the commands that take one also read it from an
//! environment variable or a file, or ask for it in a terminal (like disk passphrases do).

use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{bail, Context};
use clap::Args;

use crate::vapix::disk::secret_text;

#[derive(Args, Clone, Debug, Default)]
pub struct PwdArgs {
    /// Password. Shows in the process list and the shell history: prefer --pwd-env, --pwd-file or
    /// the prompt (asked for in a terminal when none of the three is given)
    #[arg(long, conflicts_with_all = ["pwd_env", "pwd_file"])]
    pub pwd: Option<String>,

    /// Read the password from this environment variable
    #[arg(long, value_name = "VAR", conflicts_with = "pwd_file")]
    pub pwd_env: Option<String>,

    /// Read the password from this file (mode 0600; one trailing newline is dropped)
    #[arg(long, value_name = "FILE")]
    pub pwd_file: Option<PathBuf>,
}

impl PwdArgs {
    /// The password from `--pwd`, `--pwd-env` or `--pwd-file`; None when none was given.
    pub fn given(&self) -> anyhow::Result<Option<String>> {
        let value = if let Some(p) = &self.pwd {
            eprintln!(
                "Warning: --pwd shows the password in the process list and the shell history; \
                 --pwd-env, --pwd-file or the prompt keep it out."
            );
            p.clone()
        } else if let Some(var) = &self.pwd_env {
            secret_text(None, var, "password")?
        } else if let Some(path) = &self.pwd_file {
            secret_text(Some(path), "", "password")?
        } else {
            return Ok(None);
        };
        if value.is_empty() {
            bail!("the password is empty");
        }
        Ok(Some(value))
    }

    /// The password, asked for in a terminal when none was given. `what` says whose it is, like
    /// "new account bob". With `twice` the prompt asks again to catch a typing mistake.
    pub fn read(&self, what: &str, twice: bool) -> anyhow::Result<String> {
        if let Some(p) = self.given()? {
            return Ok(p);
        }
        if !std::io::stdin().is_terminal() {
            bail!("no password for {what}: give --pwd-env or --pwd-file (or --pwd, which shows in the process list)");
        }
        let first = rpassword::prompt_password(format!("Password for {what}: ")).context("cannot read the password")?;
        if first.is_empty() {
            bail!("the password is empty");
        }
        if twice {
            let again = rpassword::prompt_password("Again: ").context("cannot read the password")?;
            if again != first {
                bail!("the two passwords differ, nothing was done");
            }
        }
        Ok(first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(pwd: Option<&str>, env: Option<&str>, file: Option<PathBuf>) -> PwdArgs {
        PwdArgs { pwd: pwd.map(Into::into), pwd_env: env.map(Into::into), pwd_file: file }
    }

    #[test]
    fn nothing_given_is_none() {
        assert_eq!(args(None, None, None).given().unwrap(), None);
    }

    #[test]
    fn the_value_comes_from_the_environment_variable() {
        std::env::set_var("VAPX_TEST_PWD_ENV", "from-env\n");
        assert_eq!(args(None, Some("VAPX_TEST_PWD_ENV"), None).given().unwrap().as_deref(), Some("from-env"));
        assert!(args(None, Some("VAPX_TEST_PWD_ENV_MISSING"), None).given().is_err());
    }

    #[test]
    #[cfg(unix)]
    fn the_value_comes_from_a_private_file_and_not_from_an_open_one() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("vapx-pwd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("pwd");
        std::fs::write(&file, "from-file\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let err = args(None, None, Some(file.clone())).given().unwrap_err().to_string();
        assert!(err.contains("readable by others"), "{err}");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(args(None, None, Some(file)).given().unwrap().as_deref(), Some("from-file"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_empty_password_is_refused() {
        std::env::set_var("VAPX_TEST_PWD_EMPTY", "");
        assert!(args(None, Some("VAPX_TEST_PWD_EMPTY"), None).given().is_err());
    }

    #[test]
    fn without_a_terminal_and_without_a_value_there_is_an_error_that_names_the_ways() {
        // stdin is not a terminal under `cargo test`
        let err = args(None, None, None).read("account bob", true).unwrap_err().to_string();
        assert!(err.contains("--pwd-env") && err.contains("--pwd-file"), "{err}");
    }
}
