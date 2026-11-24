//! `enroll` creates an account whose password nobody has seen yet. It must not print that
//! password in an error message (it ends up in logs and CI output), and the files that hold it
//! are private from the first byte. Needs no camera: a local server stands in for a
//! factory-default one.

#![cfg(unix)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;

/// `login_works`: whether the new account can read the device properties afterwards.
fn camera(login_works: bool) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            thread::spawn(move || {
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    return;
                }
                let head = String::from_utf8_lossy(&buf[..n]).into_owned();
                let line = head.lines().next().unwrap_or("");
                let (status, body) = if line.contains("systemready.cgi") {
                    ("200 OK", r#"{"apiVersion":"1.4","data":{"systemready":"yes","needsetup":"yes","passphrasepolicy":"none"}}"#)
                } else if line.contains("pwdgrp.cgi") {
                    ("200 OK", "Created account root.")
                } else if line.contains("basicdeviceinfo.cgi") && login_works {
                    ("200 OK", r#"{"apiVersion":"1.0","data":{"propertyList":{"ProdNbr":"P0000","SerialNumber":"TESTSERIAL01","Version":"11.0.0"}}}"#)
                } else {
                    ("500 Internal Server Error", "")
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
    port
}

struct Setup {
    dir: PathBuf,
    config: PathBuf,
}

fn setup(tag: &str, config_writable: bool) -> Setup {
    let dir = std::env::temp_dir().join(format!("vapx-enroll-{}-{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let config = if config_writable {
        dir.join("cameras.yaml")
    } else {
        // A directory where the config file should be: it exists, so it is the one used, and it
        // can never be written (not even by root).
        std::fs::create_dir(dir.join("cameras.yaml")).unwrap();
        dir.join("cameras.yaml")
    };
    Setup { dir, config }
}

fn enroll(s: &Setup, port: u16, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vapx"))
        .args(["--config", s.config.to_str().unwrap()])
        .args(["enroll", "127.0.0.1", "--port", &port.to_string()])
        .args(extra)
        .env("XDG_CONFIG_HOME", s.dir.join("xdg"))
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn text(out: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// The one recovery file under the config directory, and the password in it.
fn recovered(s: &Setup) -> (PathBuf, String) {
    let dir = s.dir.join("xdg").join("vapx");
    let file = std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("no recovery directory {}: {e}", dir.display())).next().unwrap().unwrap().path();
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    (file, json["password"].as_str().unwrap().to_string())
}

#[test]
fn a_failed_login_does_not_print_the_generated_password() {
    let s = setup("login", true);
    let out = enroll(&s, camera(false), &[]);
    assert!(!out.status.success(), "{}", text(&out));
    let (file, password) = recovered(&s);
    assert!(password.len() >= 12, "a generated password was expected, got {password:?}");
    assert!(!text(&out).contains(&password), "the password must not be in the output: {}", text(&out));
    assert!(text(&out).contains(file.to_str().unwrap()), "the message must name the file: {}", text(&out));
    assert_eq!(mode(&file), 0o600);
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn a_failed_config_write_does_not_print_the_generated_password() {
    let s = setup("config", false);
    let out = enroll(&s, camera(true), &[]);
    let (file, password) = recovered(&s);
    assert!(!text(&out).contains(&password), "the password must not be in the output: {}", text(&out));
    assert!(text(&out).contains(file.to_str().unwrap()), "the message must name the file: {}", text(&out));
    assert!(text(&out).contains("config was not updated"), "{}", text(&out));
    assert_eq!(mode(&file), 0o600);
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn a_failed_config_write_points_at_the_out_file_when_there_is_one() {
    let s = setup("config-out", false);
    let creds = s.dir.join("creds.json");
    let out = enroll(&s, camera(true), &["--out", creds.to_str().unwrap()]);
    let password = serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&creds).unwrap()).unwrap()["password"].as_str().unwrap().to_string();
    assert!(!text(&out).contains(&password), "{}", text(&out));
    assert!(text(&out).contains(creds.to_str().unwrap()), "{}", text(&out));
    assert!(!s.dir.join("xdg").exists(), "no second copy of the password is needed");
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn the_out_file_is_private_even_when_it_was_open_before() {
    let s = setup("out", true);
    let creds = s.dir.join("creds.json");
    std::fs::write(&creds, "old").unwrap();
    std::fs::set_permissions(&creds, std::fs::Permissions::from_mode(0o644)).unwrap();
    let out = enroll(&s, camera(true), &["--out", creds.to_str().unwrap()]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(mode(&creds), 0o600);
    assert!(std::fs::read_to_string(&creds).unwrap().contains("\"password\""));
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn a_password_from_a_file_works_and_stays_off_the_command_line() {
    let s = setup("pwdfile", true);
    let pwd_file = s.dir.join("pwd");
    std::fs::write(&pwd_file, "Chosen-Password-123\n").unwrap();
    std::fs::set_permissions(&pwd_file, std::fs::Permissions::from_mode(0o600)).unwrap();
    let creds = s.dir.join("creds.json");
    let out = enroll(&s, camera(true), &["--pwd-file", pwd_file.to_str().unwrap(), "--out", creds.to_str().unwrap()]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(std::fs::read_to_string(&creds).unwrap().contains("Chosen-Password-123"));
    assert!(!text(&out).contains("Warning: --pwd"), "{}", text(&out));
    let _ = std::fs::remove_dir_all(&s.dir);
}
