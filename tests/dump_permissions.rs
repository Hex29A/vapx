//! `params backup`-style dumps hold every parameter of a camera, passwords
//! included, so they are written readable by the owner only. Needs no camera.

#![cfg(unix)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::thread;

const PARAMS: &str = "root.Brand.Brand=AXIS\nroot.Network.Wifi.Password=hunter2\n";

/// A camera that answers every request with the parameter list.
fn camera() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            thread::spawn(move || {
                let mut buf = [0u8; 4096];
                if stream.read(&mut buf).unwrap_or(0) == 0 {
                    return;
                }
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{PARAMS}",
                    PARAMS.len()
                );
            });
        }
    });
    port
}

fn workdir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("vapx-dump-{}-{}", std::process::id(), tag));
    std::fs::create_dir_all(&dir).unwrap();
    // `--config` must name a file that exists.
    std::fs::write(dir.join("cameras.yaml"), "cameras:\n  cam1:\n    host: 192.0.2.10\n").unwrap();
    dir
}

fn mode(path: &std::path::Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn run(args: &[&str], dir: &std::path::Path, port: u16) {
    let out = Command::new(env!("CARGO_BIN_EXE_vapx"))
        .args(["--config", dir.join("cameras.yaml").to_str().unwrap()])
        .args(args)
        .args(["127.0.0.1", "-u", "root", "-p", "pass", "--port", &port.to_string()])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn backup_save_writes_a_private_file_even_over_an_open_one() {
    let (port, dir) = (camera(), workdir("backup"));
    let file = dir.join("dump.json");
    std::fs::write(&file, "old").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    run(&["backup", "save", "--output", file.to_str().unwrap()], &dir, port);
    assert!(std::fs::read_to_string(&file).unwrap().contains("hunter2"));
    assert_eq!(mode(&file), 0o600);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn template_create_writes_a_private_file() {
    let (port, dir) = (camera(), workdir("template"));
    let file = dir.join("template.yaml");
    run(&["template", "create", "--output", file.to_str().unwrap()], &dir, port);
    assert!(std::fs::read_to_string(&file).unwrap().contains("hunter2"));
    assert_eq!(mode(&file), 0o600);
    let _ = std::fs::remove_dir_all(&dir);
}
