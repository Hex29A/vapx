//! Removing accounts needs confirmation and must not lock the user out (#101). A local server
//! stands in for the camera; no camera is needed.

#![cfg(unix)]

use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::net::TcpListener;
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

type Log = Arc<Mutex<Vec<String>>>;

const USERS: &str = "root=\"root,boss\"\nadmin=\"root,boss,stsuser\"\noperator=\"root,boss,ops\"\nviewer=\"root,boss,ops,view\"\nusers=\"root,boss,ops,view,stsuser\"\n";
const ONE_ADMIN: &str = "root=\"solo\"\nadmin=\"solo,stsuser\"\nviewer=\"solo,view\"\nusers=\"solo,view,stsuser\"\n";

/// A camera: `GET pwdgrp.cgi?action=get` answers with `users`, the SSH account list with two
/// accounts, anything else with "OK" (or 404 for discovery). Every request is logged.
fn camera(users: &'static str) -> (u16, Log) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let log: Log = Default::default();
    let shared = log.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let log = shared.clone();
            thread::spawn(move || {
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    return;
                }
                let text = String::from_utf8_lossy(&buf[..n]).into_owned();
                let line = text.lines().next().unwrap_or("").to_string();
                log.lock().unwrap().push(text.clone());
                let target = line.split_whitespace().nth(1).unwrap_or("");
                let (status, body) = if target.contains("action=get") {
                    ("200 OK", users.to_string())
                } else if target.contains("/ssh/") && line.starts_with("GET") && target.ends_with("/users") {
                    ("200 OK", r#"{"status":"success","data":[{"username":"deploy"},{"username":"backup"}]}"#.to_string())
                } else if target.contains("/ssh/") && line.starts_with("GET") {
                    ("404 Not Found", String::new())
                } else {
                    ("200 OK", "OK".to_string())
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
    (port, log)
}

fn run(args: &[&str], port: u16, login: &str) -> Output {
    let dir = std::env::temp_dir().join(format!("vapx-accounts-{}-{}", std::process::id(), port));
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("cameras.yaml");
    std::fs::write(&config, "cameras:\n  cam1:\n    host: 192.0.2.10\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_vapx"))
        .args(["--config", config.to_str().unwrap()])
        .args(args)
        .args(["127.0.0.1", "-u", login, "-p", "pass", "--port", &port.to_string()])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    out
}

fn removals(log: &Log) -> usize {
    log.lock().unwrap().iter().filter(|l| l.contains("action=remove") || l.starts_with("DELETE")).count()
}

fn err(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn user_remove_without_yes_does_nothing() {
    let (port, log) = camera(USERS);
    let out = run(&["user", "remove", "--name", "view"], port, "root");
    assert!(!out.status.success());
    assert!(err(&out).contains("--yes"), "{}", err(&out));
    assert_eq!(removals(&log), 0);
}

#[test]
fn user_remove_with_yes_removes_once() {
    let (port, log) = camera(USERS);
    let out = run(&["user", "remove", "--name", "view", "--yes"], port, "root");
    assert!(out.status.success(), "{}", err(&out));
    assert_eq!(removals(&log), 1);
}

#[test]
fn the_account_vapx_logs_in_with_is_not_removed_without_force() {
    let (port, log) = camera(USERS);
    let out = run(&["user", "remove", "--name", "root", "--yes"], port, "root");
    assert!(!out.status.success());
    assert!(err(&out).contains("logged in") && err(&out).contains("--force"), "{}", err(&out));
    assert_eq!(removals(&log), 0);

    let (port, log) = camera(USERS);
    let out = run(&["user", "remove", "--name", "root", "--yes", "--force"], port, "root");
    assert!(out.status.success(), "{}", err(&out));
    assert_eq!(removals(&log), 1);
}

#[test]
fn the_last_admin_is_not_removed_without_force() {
    let (port, log) = camera(ONE_ADMIN);
    let out = run(&["user", "remove", "--name", "solo", "--yes"], port, "someone-else");
    assert!(!out.status.success());
    assert!(err(&out).contains("last admin"), "{}", err(&out));
    assert_eq!(removals(&log), 0);
}

#[test]
fn user_remove_dry_run_says_what_would_happen_and_removes_nothing() {
    let (port, log) = camera(USERS);
    let out = run(&["user", "remove", "--name", "root", "--dry-run"], port, "root");
    assert!(out.status.success(), "{}", err(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"isLoginAccount\": true") && stdout.contains("\"performed\": false"), "{stdout}");
    assert_eq!(removals(&log), 0);
}

#[test]
fn removing_an_account_that_does_not_exist_names_the_ones_that_do() {
    let (port, log) = camera(USERS);
    let out = run(&["user", "remove", "--name", "ghost", "--yes"], port, "root");
    assert!(!out.status.success());
    assert!(err(&out).contains("ops") && !err(&out).contains("stsuser"), "{}", err(&out));
    assert_eq!(removals(&log), 0);
}

#[test]
fn ssh_users_remove_needs_yes() {
    let (port, log) = camera(USERS);
    let out = run(&["ssh", "users", "remove", "--name", "deploy"], port, "root");
    assert!(!out.status.success());
    assert!(err(&out).contains("--yes"), "{}", err(&out));
    assert_eq!(removals(&log), 0);

    let (port, log) = camera(USERS);
    let out = run(&["ssh", "users", "remove", "--name", "deploy", "--yes"], port, "root");
    assert!(out.status.success(), "{}", err(&out));
    assert_eq!(removals(&log), 1);
}

#[test]
fn ssh_users_remove_dry_run_and_unknown_account() {
    let (port, log) = camera(USERS);
    let out = run(&["ssh", "users", "remove", "--name", "deploy", "--dry-run"], port, "root");
    assert!(out.status.success(), "{}", err(&out));
    assert_eq!(removals(&log), 0);

    let out = run(&["ssh", "users", "remove", "--name", "ghost", "--yes"], port, "root");
    assert!(!out.status.success());
    assert!(err(&out).contains("deploy") && err(&out).contains("backup"), "{}", err(&out));
    assert_eq!(removals(&log), 0);
}

// ---- where a password comes from (#101) ----

fn run_env(args: &[&str], port: u16, env: &[(&str, &str)]) -> Output {
    let dir = std::env::temp_dir().join(format!("vapx-accounts-env-{}-{}", std::process::id(), port));
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("cameras.yaml");
    std::fs::write(&config, "cameras:\n  cam1:\n    host: 192.0.2.10\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_vapx"))
        .args(["--config", config.to_str().unwrap()])
        .args(args)
        .args(["127.0.0.1", "-u", "root", "-p", "pass", "--port", &port.to_string()])
        .envs(env.iter().copied())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    out
}

fn sent(log: &Log, needle: &str) -> bool {
    log.lock().unwrap().iter().any(|r| r.contains(needle))
}

#[test]
fn a_new_users_password_can_come_from_the_environment() {
    let (port, log) = camera(USERS);
    let out = run_env(&["user", "add", "--name", "newbie", "--pwd-env", "VAPX_TEST_NEW_PWD"], port, &[("VAPX_TEST_NEW_PWD", "env-secret-1\n")]);
    assert!(out.status.success(), "{}", err(&out));
    assert!(sent(&log, "pwd=env-secret-1"), "the password must reach the camera in the POST body: {:?}", log.lock().unwrap());
    assert!(!err(&out).contains("--pwd shows"), "no warning for the safe route: {}", err(&out));
}

#[test]
fn a_new_users_password_can_come_from_a_private_file_but_not_an_open_one() {
    let dir = std::env::temp_dir().join(format!("vapx-accounts-file-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("pwd");
    std::fs::write(&file, "file-secret-2\n").unwrap();

    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    let (port, log) = camera(USERS);
    let out = run(&["user", "add", "--name", "newbie", "--pwd-file", file.to_str().unwrap()], port, "root");
    assert!(!out.status.success());
    assert!(err(&out).contains("readable by others"), "{}", err(&out));
    assert!(!sent(&log, "action=add"), "nothing may be sent");

    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    let (port, log) = camera(USERS);
    let out = run(&["ssh", "users", "add", "--name", "newbie", "--pwd-file", file.to_str().unwrap()], port, "root");
    assert!(out.status.success(), "{}", err(&out));
    assert!(sent(&log, "file-secret-2"), "{:?}", log.lock().unwrap());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_password_on_the_command_line_still_works_but_warns() {
    let (port, log) = camera(USERS);
    let out = run(&["user", "update", "--name", "view", "--pwd", "cli-secret-3"], port, "root");
    assert!(out.status.success(), "{}", err(&out));
    assert!(sent(&log, "cli-secret-3"));
    assert!(err(&out).contains("process list"), "{}", err(&out));
}

#[test]
fn without_a_password_and_without_a_terminal_nothing_is_sent() {
    for args in [
        &["user", "add", "--name", "newbie"][..],
        &["user", "update", "--name", "view"][..],
        &["ssh", "users", "add", "--name", "newbie"][..],
        &["ssh", "users", "passwd", "--name", "deploy"][..],
        &["pass", "--name", "root"][..],
    ] {
        let (port, log) = camera(USERS);
        let out = run(args, port, "root");
        assert!(!out.status.success(), "{args:?}");
        assert!(err(&out).contains("--pwd-env"), "{args:?}: {}", err(&out));
        assert!(!sent(&log, "action=add") && !sent(&log, "action=update") && !sent(&log, "PATCH") && !sent(&log, "POST"), "{args:?}: {:?}", log.lock().unwrap());
    }
}

#[test]
fn two_password_sources_at_once_are_refused() {
    let (port, _) = camera(USERS);
    let out = run(&["user", "add", "--name", "x", "--pwd", "a", "--pwd-env", "B"], port, "root");
    assert!(!out.status.success());
    assert!(err(&out).contains("cannot be used with"), "{}", err(&out));
}
