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
    let cmd = Command::new(env!("CARGO_BIN_EXE_vapx"));
    cmd
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

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("Invalid JSON output: {}\nstdout: {}", e, stdout));

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
    assert!(json.get("Brand").is_some());
    assert!(json.get("Version").is_some());
    assert!(json.get("ProdNbr").is_some());
    // Should NOT contain fields we didn't ask for
    assert!(json.get("Architecture").is_none());
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
    // Run in a temp dir where there's no cameras.yaml
    let output = vapx_bin()
        .args(["config", "path"])
        .env_remove("VAPX_CONFIG")
        .current_dir(std::env::temp_dir())
        .output()
        .expect("failed to run vapx");

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
    assert!(stdout.contains("1 cameras configured"));

    // Test config list
    let output = vapx_bin()
        .args(["config", "list"])
        .env("VAPX_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run vapx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("testcam"));
    assert!(stdout.contains("192.168.7.10"));

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
    assert_eq!(json["Brand"].as_str().unwrap(), "AXIS");

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
    assert_eq!(json["Brand"].as_str().unwrap(), "AXIS");

    std::fs::remove_dir_all(&tmp).ok();
}
