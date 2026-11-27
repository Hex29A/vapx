//! The env file that holds camera passwords outside cameras.yaml.
//!
//! cameras.yaml can say `pass: "${VAPX_PASS_FRONT}"`; the value then lives in
//! a `NAME=value` file that only its owner can read, loaded into the
//! environment by a wrapper script (`set -a; . env`) or by
//! `docker --env-file`. Those two read values differently (the shell parses
//! quotes, Docker does not), so a value is only written when it means the same
//! to both: unquoted, from a character set with no shell meaning.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};

/// The env file next to a cameras.yaml.
pub fn default_path(config: &Path) -> PathBuf {
    config.parent().map(|d| d.join("env")).unwrap_or_else(|| PathBuf::from("env"))
}

/// An environment variable name: a letter or underscore, then letters,
/// digits and underscores.
pub fn validate_name(name: &str) -> anyhow::Result<()> {
    let mut chars = name.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if !first_ok || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        bail!("'{}' is not a valid environment variable name (letters, digits and _, not starting with a digit)", name);
    }
    Ok(())
}

/// Whether `value` reads the same unquoted in a shell and in a Docker env
/// file. The enroll password generator only uses these characters.
pub fn value_is_safe(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|c| c.is_ascii_alphanumeric() || "-_.+=:@%#,/".contains(c))
}

/// The names defined in the env file. A file that does not exist defines none.
pub fn names(path: &Path) -> anyhow::Result<Vec<String>> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
    };
    Ok(text
        .lines()
        .map(str::trim_start)
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.strip_prefix("export ").unwrap_or(l))
        .filter_map(|l| l.split_once('=').map(|(k, _)| k.trim().to_string()))
        .filter(|k| !k.is_empty())
        .collect())
}

/// `VAPX_PASS_<CAMERA>`, with a numeric suffix if that name is taken.
pub fn derive_name(camera: &str, taken: &[String]) -> String {
    let base: String = camera
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
        .collect();
    let base = format!("VAPX_PASS_{}", base);
    if !taken.contains(&base) {
        return base;
    }
    (2..).map(|n| format!("{}_{}", base, n)).find(|n| !taken.contains(n)).expect("an unused suffix exists")
}

/// Append `NAME=value` to the env file. The file is created with mode 0600,
/// and an existing file is restricted to its owner. Nothing already in the
/// file is rewritten: the line is appended, so a failure cannot lose the other
/// passwords. A name that is already defined is an error.
pub fn append(path: &Path, name: &str, value: &str) -> anyhow::Result<()> {
    validate_name(name)?;
    if !value_is_safe(value) {
        bail!("the password has characters that a shell and a Docker env file read differently");
    }
    if names(path)?.iter().any(|n| n == name) {
        bail!("{} is already defined in {}", name, path.display());
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }

    let mut options = std::fs::OpenOptions::new();
    options.read(true).append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).with_context(|| format!("cannot write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("cannot restrict {}", path.display()))?;
    }

    // Start on a new line if the file does not end with one.
    let mut line = String::new();
    let len = file.metadata()?.len();
    if len > 0 {
        let mut last = [0u8; 1];
        file.seek(SeekFrom::End(-1))?;
        file.read_exact(&mut last)?;
        if last[0] != b'\n' {
            line.push('\n');
        }
    }
    line.push_str(&format!("{}={}\n", name, value));
    file.write_all(line.as_bytes()).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vapx-envfile-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("env")
    }

    #[test]
    fn append_creates_a_private_file() {
        let path = temp("create");
        append(&path, "VAPX_PASS_FRONT", "Ab3-xY9#z").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "VAPX_PASS_FRONT=Ab3-xY9#z\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn append_keeps_what_is_there_and_adds_a_missing_newline() {
        let path = temp("keep");
        std::fs::write(&path, "# camera passwords\nVAPX_PASS_A=one").unwrap();
        append(&path, "VAPX_PASS_B", "two").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# camera passwords\nVAPX_PASS_A=one\nVAPX_PASS_B=two\n");
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn append_refuses_a_name_that_exists() {
        let path = temp("exists");
        std::fs::write(&path, "export VAPX_PASS_A=one\n").unwrap();
        let err = append(&path, "VAPX_PASS_A", "two").unwrap_err();
        assert!(err.to_string().contains("already defined"), "{err}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "export VAPX_PASS_A=one\n");
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn append_refuses_values_a_shell_would_change() {
        let path = temp("unsafe");
        for bad in ["a$b", "a b", "a'b", "a\"b", "a`b", "a\\b", ""] {
            assert!(append(&path, "VAPX_PASS_A", bad).is_err(), "{bad:?}");
        }
        assert!(!path.exists());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn names_are_validated() {
        assert!(validate_name("VAPX_PASS_FRONT").is_ok());
        assert!(validate_name("_X1").is_ok());
        for bad in ["", "1X", "A-B", "A B", "A=B", "${A}"] {
            assert!(validate_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn derived_names_avoid_taken_ones() {
        assert_eq!(derive_name("m1075-lab", &[]), "VAPX_PASS_M1075_LAB");
        let taken = vec!["VAPX_PASS_M1075".to_string(), "VAPX_PASS_M1075_2".to_string()];
        assert_eq!(derive_name("m1075", &taken), "VAPX_PASS_M1075_3");
    }

    #[test]
    fn a_missing_file_defines_no_names() {
        let path = temp("missing");
        assert!(names(&path).unwrap().is_empty());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn generated_passwords_are_safe_in_an_env_file() {
        use crate::enroll::password;
        use crate::vapix::systemready::PassphrasePolicy;
        for _ in 0..200 {
            let pw = password::generate(PassphrasePolicy::Complex, None);
            assert!(value_is_safe(&pw), "{pw}");
        }
    }
}
