//! `vapx o3c` against a camera stand-in (#89): what each command sends, and what it
//! refuses to do. They need no camera.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::{json, Value};

type Log = Arc<Mutex<Vec<String>>>;


/// One path and the bodies it answers with, in order; the last one repeats.
struct Route(&'static str, Vec<String>);

fn route(path: &'static str, bodies: &[&str]) -> Route {
    Route(path, bodies.iter().map(|b| b.to_string()).collect())
}

/// A camera stand-in: answers by path (no authentication challenge) and records `METHOD target body`.
fn stub(routes: Vec<Route>) -> (u16, Log) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let log: Log = Default::default();
    let shared = log.clone();
    let routes = Arc::new(Mutex::new(routes));
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let (routes, log) = (routes.clone(), shared.clone());
            thread::spawn(move || {
                let mut data = Vec::new();
                let mut buf = [0u8; 4096];
                let head_end = loop {
                    let n = stream.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    data.extend_from_slice(&buf[..n]);
                    if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let head = String::from_utf8_lossy(&data[..head_end]).into_owned();
                let length = head
                    .lines()
                    .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                    .unwrap_or(0);
                while data.len() < head_end + length {
                    let n = stream.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    data.extend_from_slice(&buf[..n]);
                }
                let body = String::from_utf8_lossy(&data[head_end..]).into_owned();
                let mut first = head.lines().next().unwrap_or("").split_whitespace();
                let (method, target) = (first.next().unwrap_or(""), first.next().unwrap_or("/"));
                log.lock().unwrap().push(format!("{method} {target} {body}"));
                let path = target.split('?').next().unwrap();
                let reply = {
                    let mut routes = routes.lock().unwrap();
                    routes.iter_mut().find(|r| r.0 == path).map(|r| if r.1.len() > 1 { r.1.remove(0) } else { r.1[0].clone() })
                };
                let Some(reply) = reply else {
                    let _ = write!(stream, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    return;
                };
                let ctype = if reply.starts_with('{') { "application/json" } else { "text/plain" };
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
            });
        }
    });
    (port, log)
}

fn config(tag: &str, port: u16) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vapx-o3c-{}-{}", std::process::id(), tag));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cameras.yaml");
    std::fs::write(&path, format!("cameras:\n  cam1:\n    host: 127.0.0.1\n    port: {port}\n    user: root\n    pass: pass\n")).unwrap();
    path
}

struct Run {
    ok: bool,
    out: String,
    err: String,
    reqs: Vec<String>,
}

impl Run {
    fn json(&self) -> Value {
        serde_json::from_str(&self.out).unwrap_or_else(|e| panic!("not JSON ({e}): {}", self.out))
    }
}

fn run(tag: &str, routes: Vec<Route>, args: &[&str]) -> Run {
    let (port, log) = stub(routes);
    let cfg = config(tag, port);
    let out = Command::new(env!("CARGO_BIN_EXE_vapx"))
        .arg("--config")
        .arg(&cfg)
        .args(args)
        .arg("cam1")
        .output()
        .unwrap();
    let reqs = log.lock().unwrap().clone();
    Run {
        ok: out.status.success(),
        out: String::from_utf8_lossy(&out.stdout).into_owned(),
        err: String::from_utf8_lossy(&out.stderr).into_owned(),
        reqs,
    }
}

fn group(enabled: &str, servers: &str) -> String {
    format!(
        "root.RemoteService.Enabled={enabled}\nroot.RemoteService.ProxyPort=3128\nroot.RemoteService.ProxyServer=\nroot.RemoteService.ServerList={servers}\n"
    )
}

const DISPATCHER: &str = "dispatchse1-st.axis.com:443,dispatchse1-st.axis.com:80,192.0.2.120:443";
const SERVICE: &str = "oneclick-1.cloud.example.com:443,oneclick-2.cloud.example.com:443";
const KEY: &str = "TEST-OAK-1234";

#[test]
fn status_shows_mode_and_whether_the_camera_is_assigned() {
    let g = group("oneclick", DISPATCHER);
    let r = run("status", vec![route("/axis-cgi/param.cgi", &[&g])], &["o3c", "status"]);
    assert!(r.ok, "{}", r.err);
    let v = r.json();
    assert_eq!(v["data"]["mode"], "oneclick", "{v}");
    assert_eq!(v["data"]["servers"]["assigned"], false);
    assert_eq!(v["data"]["proxy"], Value::Null);

    let g = group("yes", SERVICE);
    let r = run("status-service", vec![route("/axis-cgi/param.cgi", &[&g])], &["cloud", "status"]);
    assert!(r.ok, "{}", r.err);
    let v = r.json();
    assert_eq!(v["data"]["mode"], "always", "{v}");
    assert_eq!(v["data"]["servers"]["domains"], json!(["cloud.example.com"]));
}

#[test]
fn mode_sets_the_enabled_parameter_and_reads_it_back() {
    let (before, after) = (group("no", DISPATCHER), group("oneclick", DISPATCHER));
    let r = run("mode", vec![route("/axis-cgi/param.cgi", &[&before, "OK", &after])], &["o3c", "mode", "oneclick"]);
    assert!(r.ok, "{}", r.err);
    let v = r.json();
    assert_eq!((v["data"]["before"].as_str(), v["data"]["after"].as_str()), (Some("off"), Some("oneclick")), "{v}");
    assert!(r.reqs.iter().any(|q| q.contains("action=update") && q.contains("root.RemoteService.Enabled=oneclick")), "{:?}", r.reqs);
}

#[test]
fn off_without_yes_changes_nothing() {
    let g = group("yes", SERVICE);
    let r = run("off", vec![route("/axis-cgi/param.cgi", &[&g])], &["o3c", "mode", "off"]);
    assert!(r.ok, "{}", r.err);
    assert_eq!(r.json()["data"]["performed"], false);
    assert!(r.reqs.iter().all(|q| !q.contains("action=update")), "{:?}", r.reqs);
}

#[test]
fn off_with_yes_sets_no() {
    let (before, after) = (group("yes", SERVICE), group("no", SERVICE));
    let r = run("off-yes", vec![route("/axis-cgi/param.cgi", &[&before, "OK", &after])], &["o3c", "mode", "off", "--yes"]);
    assert!(r.ok, "{}", r.err);
    assert!(r.reqs.iter().any(|q| q.contains("root.RemoteService.Enabled=no")), "{:?}", r.reqs);
}

#[test]
fn oak_comes_with_the_serial_number() {
    let oak = format!(r#"{{"apiVersion":"1.0","context":"vapx","method":"getOAK","data":{{"oak":"{KEY}"}}}}"#);
    let info = r#"{"apiVersion":"1.0","data":{"propertyList":{"SerialNumber":"ACCC8E000001"}}}"#;
    let r = run(
        "oak",
        vec![route("/axis-cgi/oak.cgi", &[&oak]), route("/axis-cgi/basicdeviceinfo.cgi", &[info])],
        &["o3c", "oak"],
    );
    assert!(r.ok, "{}", r.err);
    let v = r.json();
    assert_eq!((v["data"]["serial"].as_str(), v["data"]["oak"].as_str()), (Some("ACCC8E000001"), Some(KEY)), "{v}");
    assert!(r.reqs.iter().any(|q| q.starts_with("POST /axis-cgi/oak.cgi") && q.contains("getOAK")), "{:?}", r.reqs);
    assert!(!r.err.contains(KEY), "the key must not show in logs: {}", r.err);
}

#[test]
fn only_oak_prints_just_the_key() {
    let oak = format!(r#"{{"apiVersion":"1.0","method":"getOAK","data":{{"oak":"{KEY}"}}}}"#);
    let r = run("only-oak", vec![route("/axis-cgi/oak.cgi", &[&oak])], &["o3c", "oak", "--only-oak"]);
    assert!(r.ok, "{}", r.err);
    assert_eq!(r.out.trim(), KEY);
    assert!(r.reqs.iter().all(|q| !q.contains("basicdeviceinfo")), "{:?}", r.reqs);
}

#[test]
fn a_proxy_error_explains_itself() {
    let err = r#"{"apiVersion":"1.0","method":"getOAK","error":{"code":1100,"message":"Internal error"}}"#;
    let r = run("oak-1100", vec![route("/axis-cgi/oak.cgi", &[err])], &["o3c", "oak"]);
    assert!(!r.ok);
    assert!(r.err.contains("proxy"), "{}", r.err);
}

#[test]
fn a_camera_without_the_oak_api_says_where_the_key_is() {
    let r = run("oak-404", vec![], &["o3c", "oak"]);
    assert!(!r.ok);
    assert!(r.err.contains("paper"), "{}", r.err);
}
