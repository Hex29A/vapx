//! `--config FILE` names the config file, and only that one (#103). A file that does not exist
//! is an error for commands that read, and commands that write the config create it there and
//! never at the default location. Needs no camera.

#![cfg(unix)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::thread;

/// A factory-default camera that accepts the new account and lets it log in.
fn camera() -> u16 {
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
                let body = if line.contains("systemready.cgi") {
                    r#"{"apiVersion":"1.4","data":{"systemready":"yes","needsetup":"yes","passphrasepolicy":"none"}}"#
                } else if line.contains("pwdgrp.cgi") {
                    "Created account root."
                } else {
                    r#"{"apiVersion":"1.0","data":{"propertyList":{"ProdNbr":"P0000","SerialNumber":"TESTSERIAL01","Version":"11.0.0"}}}"#
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
    port
}

struct Env {
    dir: PathBuf,
}

impl Env {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("vapx-cfgflag-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Env { dir }
    }

    /// The default config location for this environment.
    fn default_config(&self) -> PathBuf {
        self.dir.join("xdg").join("vapx").join("cameras.yaml")
    }

    /// Put a config with one camera at the default location, to see that it is not used.
    fn with_default_config(self) -> Self {
        let path = self.default_config();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "cameras:\n  fallback:\n    host: 192.0.2.99\n    user: root\n    pass: x\n").unwrap();
        self
    }

    fn explicit(&self) -> PathBuf {
        self.dir.join("named").join("cameras.yaml")
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_vapx"))
            .args(args)
            .env("XDG_CONFIG_HOME", self.dir.join("xdg"))
            .env("HOME", &self.dir)
            .env_remove("VAPX_CONFIG")
            .env_remove("VAPIX_CONFIG")
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn err(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn a_missing_config_file_is_an_error_for_commands_that_read() {
    let env = Env::new("read").with_default_config();
    let missing = env.explicit();
    let missing = missing.to_str().unwrap();
    for args in [
        &["--config", missing, "config", "check"][..],
        &["--config", missing, "config", "list"][..],
        &["--config", missing, "config", "path"][..],
        &["--config", missing, "info", "fallback"][..],
        &["--config", missing, "fw", "status", "127.0.0.1", "-u", "root", "-p", "pass", "--port", "9"][..],
    ] {
        let out = env.run(args);
        assert!(!out.status.success(), "{args:?} must fail");
        assert!(err(&out).contains("does not exist") && err(&out).contains(missing), "{args:?}: {}", err(&out));
    }
}

#[test]
fn a_missing_config_file_never_falls_back_to_the_default_one() {
    let env = Env::new("nofallback").with_default_config();
    let missing = env.explicit();
    let out = env.run(&["--config", missing.to_str().unwrap(), "config", "list"]);
    assert!(!out.status.success());
    assert!(!String::from_utf8_lossy(&out.stdout).contains("fallback"), "the default config was read");
}

#[test]
fn an_existing_config_file_is_used_as_before() {
    let env = Env::new("exists").with_default_config();
    let own = env.dir.join("own.yaml");
    std::fs::write(&own, "cameras:\n  mine:\n    host: 192.0.2.7\n    user: root\n    pass: x\n").unwrap();
    let out = env.run(&["--config", own.to_str().unwrap(), "config", "list"]);
    assert!(out.status.success(), "{}", err(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("mine") && !stdout.contains("fallback"), "{stdout}");
}

#[test]
fn enroll_creates_the_named_config_and_leaves_the_default_alone() {
    let env = Env::new("enroll");
    let named = env.explicit();
    std::fs::create_dir_all(named.parent().unwrap()).unwrap();
    let port = camera().to_string();
    let out = env.run(&["--config", named.to_str().unwrap(), "enroll", "127.0.0.1", "--port", &port, "--name", "cam1", "--pwd-env", "HOME"]);
    // HOME is a path with slashes and letters, enough for a policy of `none`; the point is where the entry goes.
    assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), err(&out));
    assert!(std::fs::read_to_string(&named).unwrap().contains("cam1"), "the entry must be in the named file");
    assert!(!env.default_config().exists(), "nothing may be written at the default location");
}

#[test]
fn enroll_dry_run_names_the_file_it_would_write() {
    let env = Env::new("enroll-dry");
    let named = env.explicit();
    let port = camera().to_string();
    let out = env.run(&["--config", named.to_str().unwrap(), "enroll", "127.0.0.1", "--port", &port, "--dry-run"]);
    assert!(out.status.success(), "{}", err(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains(named.to_str().unwrap()), "{}", String::from_utf8_lossy(&out.stdout));
    assert!(!named.exists());
}

#[test]
fn config_add_and_init_create_the_named_file_and_not_the_default_one() {
    let env = Env::new("add");
    let named = env.explicit();
    let out = env.run(&["--config", named.to_str().unwrap(), "config", "add", "--name", "cam1", "--host", "192.0.2.5", "--pass", "secret", "--no-verify"]);
    assert!(out.status.success(), "{}", err(&out));
    assert!(std::fs::read_to_string(&named).unwrap().contains("cam1"));
    assert!(!env.default_config().exists());

    let other = env.dir.join("other").join("cameras.yaml");
    let out = env.run(&["--config", other.to_str().unwrap(), "config", "init"]);
    assert!(out.status.success(), "{}", err(&out));
    assert!(other.exists() && !env.default_config().exists());
}
