//! Safe, in-place editing of cameras.yaml.
//!
//! The file is hand-maintained and carries comments that matter, so entries are
//! inserted as text rather than by round-tripping through serde (which would
//! drop comments and reorder the `cameras` map). Every write is validated by
//! re-parsing the result before it replaces the original, and lands atomically
//! via a temp file in the same directory.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{bail, Context};

use crate::config::cameras::parse_config_str;

/// A camera entry to be written into cameras.yaml.
pub struct NewCamera {
    pub name: String,
    pub host: String,
    pub user: Option<String>,
    pub pass: Option<String>,
    pub https: bool,
    pub port: Option<u16>,
}

impl NewCamera {
    /// Render the entry as YAML lines, indented for the `cameras:` block.
    fn to_yaml(&self) -> String {
        let mut out = format!("  {}:\n    host: \"{}\"\n", self.name, self.host);
        if let Some(ref u) = self.user {
            out.push_str(&format!("    user: {}\n", u));
        }
        if let Some(ref p) = self.pass {
            out.push_str(&format!("    pass: \"{}\"\n", yaml_escape(p)));
        }
        if self.https {
            out.push_str("    https: true\n");
        }
        if let Some(p) = self.port {
            out.push_str(&format!("    port: {}\n", p));
        }
        out
    }
}

/// Escape a value for a YAML double-quoted scalar.
fn yaml_escape(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Axis account names are 1-14 chars of a-z A-Z 0-9; config keys are looser but
/// must still survive being a plain YAML key and a shell argument.
fn validate_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty() {
        bail!("Camera name cannot be empty");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!(
            "Camera name '{}' has invalid characters (allowed: a-z A-Z 0-9 - _)",
            name
        );
    }
    Ok(())
}

/// Find the top-level `cameras:` key. Returns its line index.
fn find_cameras_key(lines: &[&str]) -> Option<usize> {
    lines.iter().position(|l| {
        let trimmed = l.trim_end();
        trimmed == "cameras:" || trimmed == "cameras: {}" || trimmed == "cameras:{}"
    })
}

/// Index of the last line belonging to the `cameras:` block.
///
/// A line belongs to the block if it is indented; blank lines are skipped over
/// (they may sit between entries) and the first unindented non-blank line ends
/// the block. Returns the `cameras:` line itself when the block is empty.
fn cameras_block_end(lines: &[&str], start: usize) -> usize {
    let mut last = start;
    for (offset, line) in lines.iter().enumerate().skip(start + 1) {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            last = offset;
        } else {
            break;
        }
    }
    last
}

/// Insert a camera entry into cameras.yaml content, returning the new content.
///
/// This is the pure, testable core: no I/O, no validation of the parsed result.
pub fn insert_camera(content: &str, cam: &NewCamera) -> anyhow::Result<String> {
    validate_name(&cam.name)?;

    let lines: Vec<&str> = content.lines().collect();
    let entry = cam.to_yaml();

    let Some(start) = find_cameras_key(&lines) else {
        // No cameras: section at all — append one at the end.
        let mut out = content.to_string();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("cameras:\n");
        out.push_str(&entry);
        return Ok(out);
    };

    let end = cameras_block_end(&lines, start);
    let block_is_empty = end == start;

    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        // Normalize an empty flow mapping (`cameras: {}`) into a block mapping
        // so the new entry has somewhere to live.
        if i == start && line.trim_end() != "cameras:" {
            out.push_str("cameras:\n");
        } else {
            out.push_str(line);
            out.push('\n');
        }

        if i == end {
            // Separate from the preceding entry, matching the file's own style.
            if !block_is_empty {
                out.push('\n');
            }
            out.push_str(&entry);
        }
    }

    Ok(out)
}

/// Add a camera to the config file at `path`.
///
/// The result is re-parsed before it is written; if the edit would produce a
/// config that does not parse, or that loses an existing camera, nothing is
/// written. A `.bak` copy of the original is kept alongside the file.
pub fn add_camera(path: &Path, cam: &NewCamera) -> anyhow::Result<()> {
    let original = if path.exists() {
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
        String::new()
    };

    let before = parse_config_str(&original)
        .map(|c| c.cameras.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    if before.iter().any(|n| n == &cam.name) {
        bail!("Camera '{}' already exists in {}", cam.name, path.display());
    }

    let updated = insert_camera(&original, cam)?;

    // Validate before replacing anything.
    let parsed = parse_config_str(&updated).with_context(|| {
        format!(
            "Refusing to write: the edited config would not parse ({} left unchanged)",
            path.display()
        )
    })?;

    if !parsed.cameras.contains_key(&cam.name) {
        bail!(
            "Refusing to write: camera '{}' is missing from the edited config",
            cam.name
        );
    }
    for name in &before {
        if !parsed.cameras.contains_key(name) {
            bail!(
                "Refusing to write: camera '{}' would be lost by this edit",
                name
            );
        }
    }

    if !original.is_empty() {
        let backup = path.with_extension("yaml.bak");
        fs::write(&backup, &original)
            .with_context(|| format!("Failed to write backup {}", backup.display()))?;
        restrict_permissions(&backup)?;
    }

    write_atomic(path, &updated)?;

    Ok(())
}

/// Write `content` to `path` atomically: temp file in the same directory,
/// permissions tightened, then renamed over the target.
fn write_atomic(path: &Path, content: &str) -> anyhow::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = dir.join(format!(
        ".{}.vapx-tmp",
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "cameras.yaml".into())
    ));

    {
        let mut f = fs::File::create(&tmp)
            .with_context(|| format!("Failed to create {}", tmp.display()))?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
    }
    restrict_permissions(&tmp)?;

    fs::rename(&tmp, path).with_context(|| {
        format!("Failed to move {} into place at {}", tmp.display(), path.display())
    })?;

    Ok(())
}

/// The config holds passwords in plain text — keep it owner-only.
fn restrict_permissions(path: &Path) -> anyhow::Result<()> {
    let mut perms = fs::metadata(path)?.permissions();
    if perms.mode() & 0o077 != 0 {
        perms.set_mode(0o600);
        fs::set_permissions(path, perms)
            .with_context(|| format!("Failed to set permissions on {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam(name: &str) -> NewCamera {
        NewCamera {
            name: name.to_string(),
            host: "10.0.0.9".into(),
            user: None,
            pass: Some("pw".into()),
            https: false,
            port: None,
        }
    }

    /// The template `vapx config init` writes, which has profiles: between
    /// cameras: and groups: — the shape that used to corrupt the file.
    const TEMPLATE: &str = r#"# vapx camera configuration

defaults:
  user: root
  https: false

cameras:
  # my-camera:
  #   host: 192.168.1.100

profiles: {}
  # wan:
  #   timeout: 30

groups: {}
  # site-a:
  #   - my-camera
"#;

    #[test]
    fn inserts_into_cameras_block_not_profiles() {
        let out = insert_camera(TEMPLATE, &cam("newcam")).unwrap();
        let cameras_at = out.find("\ncameras:").unwrap();
        let profiles_at = out.find("\nprofiles:").unwrap();
        let entry_at = out.find("  newcam:").unwrap();
        assert!(
            entry_at > cameras_at && entry_at < profiles_at,
            "entry landed outside the cameras block:\n{}",
            out
        );
        // And the result is valid YAML with the camera present.
        let parsed = parse_config_str(&out).unwrap();
        assert_eq!(parsed.cameras["newcam"].host, "10.0.0.9");
    }

    #[test]
    fn template_result_parses() {
        // Regression: the old implementation produced YAML that failed to parse.
        let out = insert_camera(TEMPLATE, &cam("newcam")).unwrap();
        assert!(parse_config_str(&out).is_ok(), "output did not parse:\n{}", out);
    }

    #[test]
    fn preserves_existing_cameras_and_comments() {
        let yaml = r#"cameras:
  # Added 2026-07-06, replaces the old unit.
  west:
    host: "192.168.8.20"
    pass: "a"

  entren:
    host: "192.168.8.21"
    pass: "b"

groups:
  site:
    - west
"#;
        let out = insert_camera(yaml, &cam("newcam")).unwrap();
        assert!(out.contains("# Added 2026-07-06, replaces the old unit."));
        let parsed = parse_config_str(&out).unwrap();
        assert_eq!(parsed.cameras.len(), 3);
        assert!(parsed.cameras.contains_key("west"));
        assert!(parsed.cameras.contains_key("entren"));
        assert_eq!(parsed.groups["site"], vec!["west".to_string()]);
    }

    #[test]
    fn cameras_last_in_file() {
        let yaml = "defaults:\n  user: root\n\ncameras:\n  west:\n    host: \"1.2.3.4\"\n    pass: \"a\"\n";
        let out = insert_camera(yaml, &cam("newcam")).unwrap();
        let parsed = parse_config_str(&out).unwrap();
        assert_eq!(parsed.cameras.len(), 2);
    }

    #[test]
    fn empty_cameras_block_followed_by_groups() {
        let yaml = "cameras:\ngroups: {}\n";
        let out = insert_camera(yaml, &cam("newcam")).unwrap();
        let parsed = parse_config_str(&out).unwrap();
        assert_eq!(parsed.cameras.len(), 1);
        assert!(parsed.groups.is_empty());
    }

    #[test]
    fn empty_flow_mapping_is_converted_to_block() {
        let yaml = "cameras: {}\ngroups: {}\n";
        let out = insert_camera(yaml, &cam("newcam")).unwrap();
        assert!(!out.contains("cameras: {}"));
        let parsed = parse_config_str(&out).unwrap();
        assert_eq!(parsed.cameras.len(), 1);
    }

    #[test]
    fn missing_cameras_key_appends_section() {
        let yaml = "defaults:\n  user: root\n";
        let out = insert_camera(yaml, &cam("newcam")).unwrap();
        let parsed = parse_config_str(&out).unwrap();
        assert_eq!(parsed.cameras.len(), 1);
    }

    #[test]
    fn empty_file_gets_a_cameras_section() {
        let out = insert_camera("", &cam("newcam")).unwrap();
        let parsed = parse_config_str(&out).unwrap();
        assert_eq!(parsed.cameras["newcam"].host, "10.0.0.9");
    }

    #[test]
    fn file_without_trailing_newline() {
        let yaml = "cameras:\n  west:\n    host: \"1.2.3.4\"\n    pass: \"a\"";
        let out = insert_camera(yaml, &cam("newcam")).unwrap();
        let parsed = parse_config_str(&out).unwrap();
        assert_eq!(parsed.cameras.len(), 2);
    }

    #[test]
    fn writes_optional_fields() {
        let c = NewCamera {
            name: "full".into(),
            host: "1.2.3.4".into(),
            user: Some("martincr".into()),
            pass: Some("pw".into()),
            https: true,
            port: Some(8443),
        };
        let out = insert_camera("cameras:\n", &c).unwrap();
        let parsed = parse_config_str(&out).unwrap();
        let e = &parsed.cameras["full"];
        assert_eq!(e.user.as_deref(), Some("martincr"));
        assert_eq!(e.pass.as_deref(), Some("pw"));
        assert_eq!(e.https, Some(true));
        assert_eq!(e.port, Some(8443));
    }

    #[test]
    fn password_with_quotes_is_escaped() {
        let c = NewCamera {
            name: "quoted".into(),
            host: "1.2.3.4".into(),
            user: None,
            pass: Some(r#"a"b\c"#.into()),
            https: false,
            port: None,
        };
        let out = insert_camera("cameras:\n", &c).unwrap();
        let parsed = parse_config_str(&out).unwrap();
        assert_eq!(parsed.cameras["quoted"].pass.as_deref(), Some(r#"a"b\c"#));
    }

    #[test]
    fn rejects_invalid_names() {
        assert!(insert_camera("cameras:\n", &cam("bad name")).is_err());
        assert!(insert_camera("cameras:\n", &cam("bad:name")).is_err());
        assert!(insert_camera("cameras:\n", &cam("")).is_err());
    }

    #[test]
    fn add_camera_writes_validates_and_backs_up() {
        let dir = std::env::temp_dir().join(format!("vapx-writer-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cameras.yaml");
        fs::write(&path, TEMPLATE).unwrap();

        add_camera(&path, &cam("newcam")).unwrap();

        let written = fs::read_to_string(&path).unwrap();
        let parsed = parse_config_str(&written).unwrap();
        assert!(parsed.cameras.contains_key("newcam"));

        // Backup holds the original.
        let backup = fs::read_to_string(dir.join("cameras.yaml.bak")).unwrap();
        assert_eq!(backup, TEMPLATE);

        // Owner-only permissions.
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "config should be owner-only, was {:o}", mode);

        // Duplicates are refused.
        assert!(add_camera(&path, &cam("newcam")).is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_camera_creates_missing_file() {
        let dir = std::env::temp_dir().join(format!("vapx-writer-new-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("sub").join("cameras.yaml");

        add_camera(&path, &cam("newcam")).unwrap();

        let parsed = parse_config_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed.cameras.len(), 1);
        assert!(!path.with_extension("yaml.bak").exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_camera_leaves_file_untouched_when_edit_would_break_it() {
        let dir = std::env::temp_dir().join(format!("vapx-writer-bad-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cameras.yaml");
        // Already-broken file: the edit cannot produce something valid.
        let broken = "cameras:\n  west:\n   host: [unclosed\n";
        fs::write(&path, broken).unwrap();

        assert!(add_camera(&path, &cam("newcam")).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), broken);

        let _ = fs::remove_dir_all(&dir);
    }
}
