//! Regression tests for #55: reboot and factory default are sent exactly once,
//! a connection that closes after the request counts as accepted, and factory
//! default needs confirmation. Needs no camera: a local server stands in for it.

use std::io::Read;
use std::net::TcpListener;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

/// Stand-in for a camera that is going down: reads one request per connection,
/// counts it and closes the connection without answering.
fn dropping_server() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = [0u8; 4096];
            if stream.read(&mut buf).unwrap_or(0) > 0 {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        }
    });
    (port, hits)
}

fn run_vapx(args: &[&str], port: u16) -> Output {
    let dir = std::env::temp_dir().join(format!("vapx-reboot-{}-{}", std::process::id(), port));
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("cameras.yaml");
    std::fs::write(&config, "cameras:\n  cam1:\n    host: 192.0.2.10\n").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_vapx"))
        .args(["--config", config.to_str().unwrap()])
        .args(args)
        .args(["127.0.0.1", "-u", "root", "-p", "pass", "--port", &port.to_string()])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    out
}

fn assert_clean_reboot(out: &Output, hits: &AtomicUsize) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "expected success\nstdout: {stdout}\nstderr: {stderr}");
    assert_eq!(hits.load(Ordering::SeqCst), 1, "the reboot request must be sent exactly once");
    assert!(!stderr.contains("WARN"), "no warnings expected, got: {stderr}");
    assert!(!stderr.contains("Retry"), "no retries expected, got: {stderr}");
    assert!(stdout.contains("Reboot sent"), "unexpected output: {stdout}");
}

#[test]
fn fw_reboot_connection_dropped_counts_as_sent() {
    let (port, hits) = dropping_server();
    let out = run_vapx(&["fw", "reboot"], port);
    assert_clean_reboot(&out, &hits);
}

#[test]
fn top_level_reboot_is_an_alias() {
    let (port, hits) = dropping_server();
    let out = run_vapx(&["reboot"], port);
    assert_clean_reboot(&out, &hits);
}

#[test]
fn factory_default_without_confirmation_does_nothing() {
    let (port, hits) = dropping_server();
    let out = run_vapx(&["fw", "factory-default"], port);
    assert!(!out.status.success(), "must refuse without --yes when stdin is not a terminal");
    assert_eq!(hits.load(Ordering::SeqCst), 0, "no request may be sent without confirmation");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--yes"), "the error should name --yes, got: {stderr}");
}

#[test]
fn factory_default_with_yes_is_sent_once() {
    let (port, hits) = dropping_server();
    let out = run_vapx(&["fw", "factory-default", "--yes"], port);
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(hits.load(Ordering::SeqCst), 1, "factory default must be sent exactly once");
}
