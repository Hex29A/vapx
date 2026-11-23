//! `--https` makes a camera given with -u/-p use TLS. Needs no camera: a local
//! listener records the first byte it receives, which is 0x16 (a TLS
//! handshake record) for HTTPS and a letter (`GET`, `POST`) for plain HTTP.

use std::io::Read;
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn first_byte_sent(extra: &[&str]) -> u8 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
            let mut b = [0u8; 1];
            if stream.read(&mut b).unwrap_or(0) == 1 {
                let _ = tx.send(b[0]);
            }
        }
    });

    let dir = std::env::temp_dir().join(format!("vapx-https-{}-{}", std::process::id(), port));
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("cameras.yaml");
    std::fs::write(&config, "cameras:\n  cam1:\n    host: 192.0.2.10\n").unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_vapx"))
        .args(["--config", config.to_str().unwrap()])
        .args(extra)
        .args(["info", "127.0.0.1", "-u", "root", "-p", "pass", "--port", &port.to_string(), "--timeout", "2"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let byte = rx.recv_timeout(Duration::from_secs(10)).expect("the camera got no bytes");
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
    byte
}

#[test]
fn without_the_flag_a_flag_camera_is_plain_http() {
    assert!(first_byte_sent(&[]).is_ascii_uppercase());
}

#[test]
fn with_the_flag_a_flag_camera_is_spoken_to_in_tls() {
    assert_eq!(first_byte_sent(&["--https"]), 0x16);
}
