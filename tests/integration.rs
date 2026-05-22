//! Integration tests for vapx against a live Axis camera.
//!
//! These tests require a reachable camera. Set environment variables:
//!   VAPX_TEST_HOST=192.168.7.10
//!   VAPX_TEST_USER=martincr
//!   VAPX_TEST_PASS=avhsroot
//!
//! Run with: cargo test --test integration -- --nocapture

use std::process::Command;

fn vapx_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_vapx"))
}

/// Parse JSON envelope from stdout: {"status":"ok","data":...}
/// Returns the "data" field.
fn parse_ok_data(stdout: &str) -> serde_json::Value {
    let envelope: serde_json::Value = serde_json::from_str(stdout)
        .unwrap_or_else(|e| panic!("Invalid JSON: {}\nstdout: {}", e, stdout));
    assert_eq!(envelope["status"].as_str().unwrap(), "ok", "Expected status=ok, got: {}", envelope);
    envelope["data"].clone()
}

/// Parse JSON envelope from stdout: {"status":"ok","message":"..."}
/// Returns the message string.
fn parse_ok_message(stdout: &str) -> String {
    let envelope: serde_json::Value = serde_json::from_str(stdout)
        .unwrap_or_else(|e| panic!("Invalid JSON: {}\nstdout: {}", e, stdout));
    assert_eq!(envelope["status"].as_str().unwrap(), "ok", "Expected status=ok, got: {}", envelope);
    envelope["message"].as_str().unwrap().to_string()
}

fn test_host() -> String {
    std::env::var("VAPX_TEST_HOST").unwrap_or_else(|_| "192.168.7.10".into())
}

fn test_user() -> String {
    std::env::var("VAPX_TEST_USER").unwrap_or_else(|_| "martincr".into())
}

fn test_pass() -> String {
    std::env::var("VAPX_TEST_PASS").unwrap_or_else(|_| "avhsroot".into())
}

fn skip_if_no_camera() -> bool {
    // Quick TCP check
    use std::net::TcpStream;
    use std::time::Duration;
    let host = test_host();
    TcpStream::connect_timeout(
        &format!("{}:80", host).parse().unwrap(),
        Duration::from_secs(2),
    )
    .is_err()
}

#[test]
fn test_info_json_output() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args(["info", &test_host(), "-u", &test_user(), "-p", &test_pass()])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "vapx info failed: stderr={}",
        stderr
    );

    let json = parse_ok_data(&stdout);

    // Verify expected fields exist
    assert!(json.get("Brand").is_some(), "Missing Brand field");
    assert!(json.get("Version").is_some(), "Missing Version field");
    assert!(json.get("SerialNumber").is_some(), "Missing SerialNumber");
    assert!(json.get("Architecture").is_some(), "Missing Architecture");
    assert!(json.get("ProdFullName").is_some(), "Missing ProdFullName");

    // Verify known values for our test camera
    assert_eq!(json["Brand"].as_str().unwrap(), "AXIS");
    assert_eq!(json["Architecture"].as_str().unwrap(), "armv7hf");
}

#[test]
fn test_info_plain_output() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "info",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--plain",
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("Brand: AXIS"));
    assert!(stdout.contains("Architecture: armv7hf"));
    assert!(stdout.contains("Version:"));
}

#[test]
fn test_info_selective_properties() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "info",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--props",
            "Brand,Version,ProdNbr",
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let data = &json["data"];
    assert!(data.get("Brand").is_some());
    assert!(data.get("Version").is_some());
    assert!(data.get("ProdNbr").is_some());
    // Should NOT contain fields we didn't ask for
    assert!(data.get("Architecture").is_none());
}

#[test]
fn test_info_wrong_credentials() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args(["info", &test_host(), "-u", "root", "-p", "wrongpass"])
        .output()
        .expect("failed to run vapx");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("401") || stderr.contains("Unauthorized"),
        "Expected auth error, got: {}",
        stderr
    );
}

#[test]
fn test_info_unreachable_host() {
    let output = vapx_bin()
        .args(["info", "192.168.255.254", "-u", "x", "-p", "x"])
        .output()
        .expect("failed to run vapx");

    assert!(!output.status.success());
}

#[test]
fn test_config_path_no_config() {
    // Run in a temp dir where there's no cameras.yaml, with no XDG config
    let empty_dir = std::env::temp_dir().join(format!("vapx_no_config_{}", std::process::id()));
    std::fs::create_dir_all(&empty_dir).unwrap();
    let output = vapx_bin()
        .args(["config", "path"])
        .env_remove("VAPX_CONFIG")
        .env("HOME", &empty_dir)
        .current_dir(&empty_dir)
        .output()
        .expect("failed to run vapx");

    let _ = std::fs::remove_dir_all(&empty_dir);

    // Should exit with error when no config found
    assert!(!output.status.success());
}

#[test]
fn test_config_init_and_check() {
    let tmp = std::env::temp_dir().join(format!("vapx_test_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let config_path = tmp.join("cameras.yaml");

    // Create a test config
    std::fs::write(
        &config_path,
        r#"
defaults:
  user: root
  https: false
  verify_ssl: false

cameras:
  testcam:
    host: 192.168.7.10
    pass: "testpass"
"#,
    )
    .unwrap();

    // Test config check
    let output = vapx_bin()
        .args(["config", "check"])
        .env("VAPX_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "config check failed");
    let data = parse_ok_data(&stdout);
    assert_eq!(data["cameras"].as_i64().unwrap(), 1);

    // Test config list
    let output = vapx_bin()
        .args(["config", "list"])
        .env("VAPX_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    let data = parse_ok_data(&stdout);
    assert!(data.is_array());
    let cameras = data.as_array().unwrap();
    assert_eq!(cameras.len(), 1);
    assert_eq!(cameras[0]["name"].as_str().unwrap(), "testcam");
    assert_eq!(cameras[0]["host"].as_str().unwrap(), "192.168.7.10");

    // Cleanup
    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn test_config_with_camera_name_resolution() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let tmp = std::env::temp_dir().join(format!("vapx_test_name_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let config_path = tmp.join("cameras.yaml");

    std::fs::write(
        &config_path,
        format!(
            r#"
cameras:
  testcam:
    host: {}
    user: {}
    pass: "{}"
"#,
            test_host(),
            test_user(),
            test_pass()
        ),
    )
    .unwrap();

    // Use camera name instead of IP
    let output = vapx_bin()
        .args(["info", "testcam"])
        .env("VAPX_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "name resolution failed: {}",
        stderr
    );

    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["data"]["Brand"].as_str().unwrap(), "AXIS");

    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn test_env_var_substitution_in_config() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let tmp = std::env::temp_dir().join(format!("vapx_test_env_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let config_path = tmp.join("cameras.yaml");

    std::fs::write(
        &config_path,
        format!(
            r#"
cameras:
  envcam:
    host: {}
    user: "${{VAPX_TEST_USER}}"
    pass: "${{VAPX_TEST_PASS}}"
"#,
            test_host()
        ),
    )
    .unwrap();

    let output = vapx_bin()
        .args(["info", "envcam"])
        .env("VAPX_CONFIG", config_path.to_str().unwrap())
        .env("VAPX_TEST_USER", test_user())
        .env("VAPX_TEST_PASS", test_pass())
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "env var substitution failed: {}",
        stderr
    );

    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["data"]["Brand"].as_str().unwrap(), "AXIS");

    std::fs::remove_dir_all(&tmp).ok();
}

// ── Snapshot tests ──────────────────────────────────────────────────

#[test]
fn test_snap_default() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let tmp = std::env::temp_dir().join(format!("vapx_snap_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let out_file = tmp.join("test.jpg");

    let output = vapx_bin()
        .args([
            "snap",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "-o",
            out_file.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "snap failed: {}", stderr);

    assert!(out_file.exists(), "Snapshot file not created");
    let metadata = std::fs::metadata(&out_file).unwrap();
    assert!(metadata.len() > 1000, "Snapshot too small: {} bytes", metadata.len());

    let json = parse_ok_data(&stdout);
    assert!(json.get("file").is_some());
    assert!(json.get("bytes").is_some());

    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn test_snap_with_resolution() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let tmp = std::env::temp_dir().join(format!("vapx_snap_res_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let out_file = tmp.join("small.jpg");

    let output = vapx_bin()
        .args([
            "snap",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "-o",
            out_file.to_str().unwrap(),
            "--resolution",
            "320x240",
        ])
        .output()
        .expect("failed to run vapx");

    assert!(output.status.success());
    assert!(out_file.exists());

    std::fs::remove_dir_all(&tmp).ok();
}

// ── Firmware tests ──────────────────────────────────────────────────

#[test]
fn test_fw_status_json() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args(["fw", "status", &test_host(), "-u", &test_user(), "-p", &test_pass()])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "fw status failed: {}", stderr);

    let json = parse_ok_data(&stdout);
    assert!(
        json.get("activeFirmwareVersion").is_some(),
        "Missing activeFirmwareVersion"
    );
}

#[test]
fn test_fw_status_plain() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "fw",
            "status",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--plain",
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("activeFirmwareVersion:"));
}

// ── ACAP tests ──────────────────────────────────────────────────────

#[test]
fn test_acap_list_json() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "acap",
            "list",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "acap list failed: {}", stderr);

    let json = parse_ok_data(&stdout);
    assert!(json.is_array(), "Expected JSON array");
}

#[test]
fn test_acap_list_plain() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "acap",
            "list",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--plain",
        ])
        .output()
        .expect("failed to run vapx");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.starts_with('['),
        "Plain output should not be JSON array"
    );
}

// ── PTZ tests ───────────────────────────────────────────────────────

#[test]
fn test_ptz_info() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "ptz",
            "info",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "ptz info failed: {}",
        stderr
    );
    // Info returns text listing available PTZ commands
    assert!(!stdout.is_empty(), "ptz info returned empty output");
}

#[test]
fn test_ptz_query_position() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "ptz",
            "query",
            &test_host(),
            "position",
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "ptz query position failed: {}",
        stderr
    );

    let json = parse_ok_data(&stdout);
    assert!(json.is_object(), "Expected JSON object");
}

#[test]
fn test_ptz_query_limits() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "ptz",
            "query",
            &test_host(),
            "limits",
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--plain",
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(!stdout.is_empty(), "ptz query limits returned empty");
}

// ── Parameter tests ─────────────────────────────────────────────────

#[test]
fn test_param_list_brand_group() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "param",
            "list",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--group",
            "root.Brand",
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "param list failed: {}",
        stderr
    );

    let json = parse_ok_data(&stdout);
    assert!(json.is_object());
    assert!(json.get("root.Brand.Brand").is_some(), "Missing root.Brand.Brand");
    assert_eq!(json["root.Brand.Brand"].as_str().unwrap(), "AXIS");
}

#[test]
fn test_param_get_single() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "param",
            "get",
            &test_host(),
            "root.Brand.Brand",
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert_eq!(stdout.trim(), "AXIS");
}

#[test]
fn test_param_list_plain() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "param",
            "list",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--group",
            "root.Brand",
            "--plain",
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("root.Brand.Brand=AXIS"));
}

// ── User management tests ───────────────────────────────────────────

#[test]
fn test_user_list_json() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "user",
            "list",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "user list failed: {}",
        stderr
    );

    let json = parse_ok_data(&stdout);
    assert!(json.is_object());
}

#[test]
fn test_user_list_plain() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "user",
            "list",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--plain",
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(!stdout.is_empty(), "user list returned empty");
}

#[test]
fn test_user_add_update_remove_lifecycle() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let test_account = "vapxtest";

    // 1. Add user
    let output = vapx_bin()
        .args([
            "user",
            "add",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--name",
            test_account,
            "--pwd",
            "TestPass123",
            "--role",
            "viewer",
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "user add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let msg = parse_ok_message(&stdout);
    assert!(
        msg.contains("Created account"),
        "Expected 'Created account', got: {}",
        msg
    );

    // 2. Verify user appears in list
    let output = vapx_bin()
        .args([
            "user",
            "list",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--plain",
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(test_account),
        "User {} not found in list: {}",
        test_account,
        stdout
    );

    // 3. Update password
    let output = vapx_bin()
        .args([
            "user",
            "update",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--name",
            test_account,
            "--pwd",
            "NewPass456",
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "user update failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let msg = parse_ok_message(&stdout);
    assert!(
        msg.contains("Modified account"),
        "Expected 'Modified account', got: {}",
        msg
    );

    // 4. Remove user
    let output = vapx_bin()
        .args([
            "user",
            "remove",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--name",
            test_account,
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "user remove failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let msg = parse_ok_message(&stdout);
    assert!(
        msg.contains("Removed account"),
        "Expected 'Removed account', got: {}",
        msg
    );

    // 5. Verify user is gone
    let output = vapx_bin()
        .args([
            "user",
            "list",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--plain",
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains(test_account),
        "User {} still in list after removal: {}",
        test_account,
        stdout
    );
}

// ── Password management tests ───────────────────────────────────────

#[test]
fn test_pass_change_lifecycle() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let test_account = "vapxpasst";
    let initial_pwd = "InitPass1";
    let new_pwd = "NewPass42";

    // 1. Create a test user
    let output = vapx_bin()
        .args([
            "user",
            "add",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--name",
            test_account,
            "--pwd",
            initial_pwd,
            "--role",
            "viewer",
        ])
        .output()
        .expect("failed to run vapx");

    assert!(
        output.status.success(),
        "user add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 2. Change password via vapx pass
    let output = vapx_bin()
        .args([
            "pass",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--name",
            test_account,
            "--pwd",
            new_pwd,
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "pass change failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let msg = parse_ok_message(&stdout);
    assert!(
        msg.contains("Modified account"),
        "Expected 'Modified account', got: {}",
        msg
    );

    // 3. Cleanup: remove test user
    let output = vapx_bin()
        .args([
            "user",
            "remove",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--name",
            test_account,
        ])
        .output()
        .expect("failed to run vapx");

    assert!(
        output.status.success(),
        "user remove failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── Network configuration tests ─────────────────────────────────────

#[test]
fn test_net_show_json() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "net",
            "show",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "net show failed: {}",
        stderr
    );

    let json = parse_ok_data(&stdout);
    assert!(json.is_object());
    assert!(
        json.get("root.Network.IPAddress").is_some(),
        "Missing root.Network.IPAddress"
    );
    assert!(
        json.get("root.Network.BootProto").is_some(),
        "Missing root.Network.BootProto"
    );
}

#[test]
fn test_net_show_plain() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "net",
            "show",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--plain",
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(
        stdout.contains("root.Network.IPAddress"),
        "Missing IP address in plain output"
    );
}

#[test]
fn test_net_set_hostname_roundtrip() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    // 1. Read current hostname
    let output = vapx_bin()
        .args([
            "param",
            "get",
            &test_host(),
            "root.Network.HostName",
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
        ])
        .output()
        .expect("failed to run vapx");

    let original_hostname = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // 2. Set a test hostname via vapx net set
    let output = vapx_bin()
        .args([
            "net",
            "set",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--hostname",
            "vapxtest",
        ])
        .output()
        .expect("failed to run vapx");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "net set hostname failed: {}",
        stderr
    );

    // 3. Verify it changed
    let output = vapx_bin()
        .args([
            "param",
            "get",
            &test_host(),
            "root.Network.HostName",
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
        ])
        .output()
        .expect("failed to run vapx");

    let new_hostname = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(new_hostname, "vapxtest", "Hostname was not changed");

    // 4. Restore original hostname
    let output = vapx_bin()
        .args([
            "net",
            "set",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--hostname",
            &original_hostname,
        ])
        .output()
        .expect("failed to run vapx");

    assert!(
        output.status.success(),
        "Restoring hostname failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── Time/NTP tests ──────────────────────────────────────────────────

#[test]
fn test_time_show_json() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "time",
            "show",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "time show failed: {}",
        stderr
    );

    let json = parse_ok_data(&stdout);
    assert!(json.is_object());
    assert!(
        json.get("root.Time.SyncSource").is_some(),
        "Missing root.Time.SyncSource"
    );
    assert!(
        json.get("root.Time.NTP.Server").is_some(),
        "Missing root.Time.NTP.Server"
    );
}

#[test]
fn test_time_show_plain() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "time",
            "show",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--plain",
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("root.Time.SyncSource"));
}

#[test]
fn test_time_set_ntp_roundtrip() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    // 1. Read current NTP server
    let output = vapx_bin()
        .args([
            "param",
            "get",
            &test_host(),
            "root.Time.NTP.Server",
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
        ])
        .output()
        .expect("failed to run vapx");

    let original_ntp = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // 2. Set a test NTP server
    let output = vapx_bin()
        .args([
            "time",
            "set",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--ntp",
            "time.google.com",
        ])
        .output()
        .expect("failed to run vapx");

    assert!(
        output.status.success(),
        "time set failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 3. Verify it changed
    let output = vapx_bin()
        .args([
            "param",
            "get",
            &test_host(),
            "root.Time.NTP.Server",
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
        ])
        .output()
        .expect("failed to run vapx");

    let new_ntp = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(new_ntp, "time.google.com", "NTP server was not changed");

    // 4. Restore original
    let output = vapx_bin()
        .args([
            "time",
            "set",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--ntp",
            &original_ntp,
        ])
        .output()
        .expect("failed to run vapx");

    assert!(
        output.status.success(),
        "Restoring NTP failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── I/O port tests ──────────────────────────────────────────────────

#[test]
fn test_hw_show_json() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "hw",
            "show",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "hw show failed: {}",
        stderr
    );

    let json = parse_ok_data(&stdout);
    assert!(json.is_object());
    assert!(
        json.get("root.IOPort.I0.Direction").is_some(),
        "Missing root.IOPort.I0.Direction"
    );
}

#[test]
fn test_hw_show_plain() {
    if skip_if_no_camera() {
        eprintln!("SKIP: camera not reachable");
        return;
    }

    let output = vapx_bin()
        .args([
            "hw",
            "show",
            &test_host(),
            "-u",
            &test_user(),
            "-p",
            &test_pass(),
            "--plain",
        ])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("root.IOPort.I0.Direction"));
}

// ── Shell completions test ──────────────────────────────────────────

#[test]
fn test_completions_bash() {
    let output = vapx_bin()
        .args(["completions", "bash"])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("_vapx"), "Missing bash completion function");
    assert!(stdout.contains("COMPREPLY"), "Missing COMPREPLY");
}

#[test]
fn test_completions_zsh() {
    let output = vapx_bin()
        .args(["completions", "zsh"])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("vapx"), "Missing zsh completion content");
}

#[test]
fn test_completions_fish() {
    let output = vapx_bin()
        .args(["completions", "fish"])
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("vapx"), "Missing fish completion content");
}
