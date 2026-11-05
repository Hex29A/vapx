//! `vapx acap install|upgrade|license|unsigned` against a camera stand-in: what is sent,
//! what is refused before anything is sent, and what the answers become. No camera needed.

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
    let dir = std::env::temp_dir().join(format!("vapx-acap-{}-{}", std::process::id(), tag));
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


fn eap(dir: &std::path::Path, file: &str, name: &str, version: &str, arch: &str) -> String {
    let manifest = format!(
        r#"{{"schemaVersion":"1.3","acapPackageConf":{{"setup":{{"appName":"{name}","friendlyName":"Test","vendor":"Example","version":"{version}","architecture":"{arch}"}}}}}}"#
    );
    let mut b = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
    let mut h = tar::Header::new_gnu();
    h.set_size(manifest.len() as u64);
    h.set_mode(0o644);
    h.set_cksum();
    b.append_data(&mut h, "manifest.json", manifest.as_bytes()).unwrap();
    let bytes = b.into_inner().unwrap().finish().unwrap();
    let path = dir.join(file);
    std::fs::write(&path, bytes).unwrap();
    path.to_string_lossy().into_owned()
}

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("vapx-acap-eap-{}-{}", std::process::id(), tag));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn list(apps: &[(&str, &str, &str)]) -> String {
    let mut s = String::from(r#"<reply result="ok">"#);
    for (name, version, status) in apps {
        s.push_str(&format!(r#"<application Name="{name}" NiceName="Test" Vendor="Example" Version="{version}" Status="{status}" License="None" SignatureStatus="Signed"/>"#));
    }
    s.push_str("</reply>");
    s
}

const ARM: &str = "root.Properties.System.Architecture=armv7hf\n";

#[test]
fn a_dry_run_says_upgrade_from_and_to_and_sends_nothing() {
    let d = tmp("dry");
    let f = eap(&d, "hello.eap", "hello", "1.3.0", "armv7hf");
    let l = list(&[("hello", "1.2-0", "Running")]);
    let r = run("dry", vec![route("/axis-cgi/param.cgi", &[ARM]), route("/axis-cgi/applications/list.cgi", &[&l])], &["acap", "install", "--eap", &f, "--dry-run"]);
    assert!(r.ok, "{}", r.err);
    let v = r.json();
    assert_eq!(v["data"]["plan"], "upgrade", "{v}");
    assert_eq!(v["data"]["installedVersion"], "1.2-0");
    assert_eq!(v["data"]["performed"], false);
    assert!(r.reqs.iter().all(|q| !q.contains("upload.cgi")), "{:?}", r.reqs);
}

#[test]
fn install_uploads_the_file_field_and_checks_the_listed_version() {
    let d = tmp("install");
    let f = eap(&d, "hello.eap", "hello", "1.3.0", "armv7hf");
    let (before, after) = (list(&[]), list(&[("hello", "1.3-0", "Stopped")]));
    let r = run(
        "install",
        vec![
            route("/axis-cgi/param.cgi", &[ARM]),
            route("/axis-cgi/applications/list.cgi", &[&before, &after]),
            route("/axis-cgi/applications/upload.cgi", &["OK"]),
        ],
        &["acap", "install", "--eap", &f],
    );
    assert!(r.ok, "{}", r.err);
    let v = r.json();
    assert_eq!((v["data"]["plan"].as_str(), v["data"]["version"].as_str()), (Some("install"), Some("1.3-0")), "{v}");
    let up: Vec<&String> = r.reqs.iter().filter(|q| q.starts_with("POST /axis-cgi/applications/upload.cgi")).collect();
    assert_eq!(up.len(), 1, "{:?}", r.reqs);
    assert!(up[0].contains(r#"name="file"; filename="hello.eap""#), "{}", up[0]);
}

#[test]
fn start_starts_a_new_install() {
    let d = tmp("start");
    let f = eap(&d, "hello.eap", "hello", "1.0", "all");
    let (before, mid, after) = (list(&[]), list(&[("hello", "1.0", "Stopped")]), list(&[("hello", "1.0", "Running")]));
    let r = run(
        "start",
        vec![
            route("/axis-cgi/param.cgi", &[ARM]),
            route("/axis-cgi/applications/list.cgi", &[&before, &mid, &after]),
            route("/axis-cgi/applications/upload.cgi", &["OK"]),
            route("/axis-cgi/applications/control.cgi", &["OK"]),
        ],
        &["acap", "install", "--eap", &f, "--start"],
    );
    assert!(r.ok, "{}", r.err);
    assert_eq!(r.json()["data"]["status"], "Running");
    assert!(r.reqs.iter().any(|q| q.contains("control.cgi") && q.contains("action=start") && q.contains("package=hello")), "{:?}", r.reqs);
}

#[test]
fn a_downgrade_needs_allow_downgrade() {
    let d = tmp("down");
    let f = eap(&d, "hello.eap", "hello", "1.0.0", "armv7hf");
    let l = list(&[("hello", "2.0-0", "Running")]);
    let r = run("down", vec![route("/axis-cgi/param.cgi", &[ARM]), route("/axis-cgi/applications/list.cgi", &[&l])], &["acap", "install", "--eap", &f]);
    assert!(!r.ok);
    assert!(r.err.contains("--allow-downgrade"), "{}", r.err);
    assert!(r.reqs.iter().all(|q| !q.contains("upload.cgi")), "{:?}", r.reqs);

    let after = list(&[("hello", "1.0-0", "Running")]);
    let r = run(
        "down-yes",
        vec![route("/axis-cgi/param.cgi", &[ARM]), route("/axis-cgi/applications/list.cgi", &[&l, &after]), route("/axis-cgi/applications/upload.cgi", &["OK"])],
        &["acap", "install", "--eap", &f, "--allow-downgrade"],
    );
    assert!(r.ok, "{}", r.err);
    assert_eq!(r.json()["data"]["plan"], "downgrade");
}

#[test]
fn upgrade_refuses_an_app_that_is_not_installed() {
    let d = tmp("upg");
    let f = eap(&d, "hello.eap", "hello", "1.0", "armv7hf");
    let l = list(&[]);
    let r = run("upg", vec![route("/axis-cgi/param.cgi", &[ARM]), route("/axis-cgi/applications/list.cgi", &[&l])], &["acap", "upgrade", "--eap", &f]);
    assert!(!r.ok);
    assert!(r.err.contains("not installed"), "{}", r.err);
}

#[test]
fn the_package_for_the_camera_architecture_is_uploaded() {
    let d = tmp("arch");
    let a64 = eap(&d, "hello_aarch64.eap", "hello", "1.0", "aarch64");
    let arm = eap(&d, "hello_armv7hf.eap", "hello", "1.0", "armv7hf");
    let (before, after) = (list(&[]), list(&[("hello", "1.0", "Stopped")]));
    let r = run(
        "arch",
        vec![route("/axis-cgi/param.cgi", &[ARM]), route("/axis-cgi/applications/list.cgi", &[&before, &after]), route("/axis-cgi/applications/upload.cgi", &["OK"])],
        &["acap", "install", "--eap", &a64, "--eap", &arm],
    );
    assert!(r.ok, "{}", r.err);
    assert!(r.reqs.iter().any(|q| q.contains(r#"filename="hello_armv7hf.eap""#)), "{:?}", r.reqs.iter().map(|q| q.chars().take(200).collect::<String>()).collect::<Vec<_>>());

    let r = run("arch-none", vec![route("/axis-cgi/param.cgi", &[ARM])], &["acap", "install", "--eap", &a64]);
    assert!(!r.ok);
    assert!(r.err.contains("architecture armv7hf"), "{}", r.err);
}

#[test]
fn an_unsigned_package_on_a_strict_camera_explains_itself() {
    let d = tmp("sig");
    let f = eap(&d, "hello.eap", "hello", "1.0", "armv7hf");
    let l = list(&[]);
    let r = run(
        "sig",
        vec![route("/axis-cgi/param.cgi", &[ARM]), route("/axis-cgi/applications/list.cgi", &[&l]), route("/axis-cgi/applications/upload.cgi", &["Error: 2"])],
        &["acap", "install", "--eap", &f],
    );
    assert!(!r.ok);
    assert!(r.err.contains("signature") && r.err.contains("vapx acap unsigned"), "{}", r.err);
}

#[test]
fn a_file_that_is_not_a_package_is_refused_before_anything_is_sent() {
    let d = tmp("bad");
    let f = d.join("bad.eap");
    std::fs::write(&f, b"not a package").unwrap();
    let r = run("bad", vec![], &["acap", "install", "--eap", f.to_str().unwrap()]);
    assert!(!r.ok);
    assert!(r.reqs.is_empty(), "{:?}", r.reqs);
}

#[test]
fn unsigned_on_needs_yes_and_off_is_read_back() {
    let get_false = r#"<reply result="ok"><param name="AllowUnsigned" value="false" /></reply>"#;
    let get_true = r#"<reply result="ok"><param name="AllowUnsigned" value="true" /></reply>"#;
    let r = run("uns-on", vec![route("/axis-cgi/applications/config.cgi", &[get_false])], &["acap", "unsigned", "on"]);
    assert!(r.ok, "{}", r.err);
    assert_eq!(r.json()["data"]["performed"], false);
    assert!(r.reqs.iter().all(|q| !q.contains("action=set")), "{:?}", r.reqs);

    let r = run("uns-off", vec![route("/axis-cgi/applications/config.cgi", &[get_true, get_false, get_false])], &["acap", "unsigned", "off"]);
    assert!(r.ok, "{}", r.err);
    assert_eq!(r.json()["data"]["allowUnsigned"], false);
    assert!(r.reqs.iter().any(|q| q.contains("action=set") && q.contains("name=AllowUnsigned") && q.contains("value=false")), "{:?}", r.reqs);

    let r = run("uns-status", vec![route("/axis-cgi/applications/config.cgi", &[get_true])], &["acap", "unsigned", "status"]);
    assert!(r.ok, "{}", r.err);
    assert_eq!(r.json()["data"]["allowUnsigned"], true);
}

#[test]
fn a_license_key_is_uploaded_for_an_installed_app() {
    let d = tmp("lic");
    let key = d.join("key.xml");
    std::fs::write(&key, "<license/>").unwrap();
    let l = list(&[("hello", "1.0", "Running")]);
    let r = run(
        "lic",
        vec![route("/axis-cgi/applications/list.cgi", &[&l]), route("/axis-cgi/applications/license.cgi", &["OK"])],
        &["acap", "license", "--app", "hello", "--key", key.to_str().unwrap()],
    );
    assert!(r.ok, "{}", r.err);
    assert!(r.reqs.iter().any(|q| q.starts_with("POST /axis-cgi/applications/license.cgi") && q.contains("action=uploadlicensekey") && q.contains("package=hello")), "{:?}", r.reqs);

    let r = run("lic-err", vec![route("/axis-cgi/applications/list.cgi", &[&l]), route("/axis-cgi/applications/license.cgi", &["Error: 30"])], &["acap", "license", "--app", "hello", "--key", key.to_str().unwrap()]);
    assert!(!r.ok);
    assert!(r.err.contains("different camera"), "{}", r.err);
}
