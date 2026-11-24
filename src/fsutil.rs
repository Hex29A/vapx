//! Files that hold secrets: parameter dumps, recovery passwords.

use std::io::Write;
use std::path::Path;

use anyhow::Context;

/// Write `bytes` to a file only its owner can read (mode 0600 on Unix).
///
/// The file is never readable by others, not even for a moment: it is created
/// with mode 0600, and a file that already exists with a looser mode is
/// restricted before the old content is replaced. On Windows the user profile's
/// default ACL applies.
pub fn write_private(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    // No truncate here: that would empty a file before its mode is tightened.
    options.write(true).create(true).truncate(false);
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
    file.set_len(0).with_context(|| format!("cannot write {}", path.display()))?;
    file.write_all(bytes).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("vapx-fsutil-{}-{}", std::process::id(), name));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("dump.json")
    }

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    #[cfg(unix)]
    fn a_new_file_is_private() {
        let path = temp("new");
        write_private(&path, b"secret").unwrap();
        assert_eq!(mode(&path), 0o600);
        assert_eq!(std::fs::read(&path).unwrap(), b"secret");
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn an_existing_open_file_is_tightened_and_replaced() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp("old");
        std::fs::write(&path, "a much longer old content").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_private(&path, b"new").unwrap();
        assert_eq!(mode(&path), 0o600);
        assert_eq!(std::fs::read(&path).unwrap(), b"new", "the old content must be gone");
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
