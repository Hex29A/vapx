//! End-to-end tests for `vapx mcp` (#57): stdio and HTTP transports, token auth,
//! tool listing and a camera-less tool call. They need no camera.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const READ_TOOLS: &[&str] = &[
    "list_cameras", "get_camera_info", "get_snapshot", "status_all", "get_temperature", "get_time_info",
    "get_ntp_status", "get_system_log", "get_audit_log", "get_users", "list_applications",
    "list_stream_profiles", "get_stream_status", "get_io_ports", "get_disk_status", "list_recordings",
    "get_recording_info", "get_daynight_config", "get_motion_config", "get_lights", "get_mqtt_config",
    "list_overlays", "list_privacy_masks", "list_view_areas", "get_capture_modes", "get_orientation",
    "get_geolocation", "get_signed_video_status", "get_audio_settings", "list_audio_clips",
    "list_guard_tours", "ptz_status", "get_siren_status", "get_clear_view_info",
    "list_analytics_producers", "check_systemready", "discover_apis",
];

fn test_config(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vapx-mcp-{}-{}", std::process::id(), tag));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cameras.yaml");
    std::fs::write(
        &path,
        "cameras:\n  front:\n    host: 192.0.2.10\n    user: root\n    pass: pass\n  \
         yard:\n    host: 192.0.2.11\n    user: root\n    pass: pass\n",
    )
    .unwrap();
    path
}

fn vapx(config: &PathBuf) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_vapx"));
    cmd.arg("--config").arg(config);
    cmd
}

// ---- stdio ----

struct Stdio_ {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
}

impl Stdio_ {
    fn start(config: &PathBuf) -> Self {
        let mut child = vapx(config)
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self { child, stdin, lines }
    }

    fn send(&mut self, msg: Value) {
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }

    /// The response with this id, skipping notifications.
    fn response(&mut self, id: i64) -> Value {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = self.lines.recv_timeout(left).expect("no response from vapx mcp");
            let v: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON-RPC: {e}: {line}"));
            if v["id"] == json!(id) {
                return v;
            }
        }
    }

    fn initialize(&mut self) {
        self.send(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-03-26", "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}}}));
        let r = self.response(1);
        assert_eq!(r["result"]["serverInfo"]["name"], "vapx", "{r}");
        self.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    }
}

impl Drop for Stdio_ {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn assert_tools_listed(result: &Value) {
    let tools = result["tools"].as_array().expect("tools array");
    for want in READ_TOOLS {
        let tool = tools.iter().find(|t| t["name"] == *want).unwrap_or_else(|| panic!("missing tool {want}"));
        assert_eq!(tool["inputSchema"]["type"], "object", "{want} has no object schema");
    }
}

#[test]
fn stdio_lists_tools_and_answers_list_cameras() {
    let mut s = Stdio_::start(&test_config("stdio"));
    s.initialize();

    s.send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    assert_tools_listed(&s.response(2)["result"]);

    s.send(json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                  "params": {"name": "list_cameras", "arguments": {}}}));
    let r = s.response(3);
    assert_ne!(r["result"]["isError"], json!(true), "{r}");
    let text = r["result"]["content"][0]["text"].as_str().expect("text content");
    let cams: Value = serde_json::from_str(text).unwrap();
    let ids: Vec<&str> = cams.as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["front", "yard"]);
    assert_eq!(cams[0]["host"], "192.0.2.10");
    assert!(cams[0].get("pass").is_none() && !text.contains("pass"), "the password must never be listed");
}

#[test]
fn stdio_reports_unknown_cameras_and_tools_as_errors() {
    let mut s = Stdio_::start(&test_config("stdio-err"));
    s.initialize();

    s.send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                  "params": {"name": "get_camera_info", "arguments": {"camera_id": "nope"}}}));
    let r = s.response(2);
    assert_eq!(r["result"]["isError"], json!(true), "{r}");
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.starts_with("Error: ") && text.contains("front, yard"), "{text}");

    s.send(json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                  "params": {"name": "get_camera_info", "arguments": {}}}));
    let text = s.response(3)["result"]["content"][0]["text"].as_str().unwrap().to_string();
    assert!(text.contains("camera_id is required"), "{text}");

    s.send(json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
                  "params": {"name": "format_disk", "arguments": {}}}));
    let r = s.response(4);
    assert_eq!(r["result"]["isError"], json!(true), "{r}");
}

// ---- HTTP ----

struct Http {
    child: Child,
    url: String,
    client: reqwest::blocking::Client,
}

fn free_addr() -> SocketAddr {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap()
}

impl Http {
    fn start(config: &PathBuf, token: &str) -> Self {
        let addr = free_addr();
        let child = vapx(config)
            .args(["mcp", "--transport", "http", "--bind", &addr.to_string()])
            .env("VAPX_MCP_TOKEN", token)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        while TcpStream::connect(addr).is_err() {
            assert!(Instant::now() < deadline, "vapx mcp did not start listening");
            thread::sleep(Duration::from_millis(50));
        }
        Self { child, url: format!("http://{addr}/mcp"), client: reqwest::blocking::Client::new() }
    }

    fn post(&self, token: Option<&str>, body: Value) -> reqwest::blocking::Response {
        let mut req = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(&body);
        if let Some(t) = token {
            req = req.header("Authorization", format!("Bearer {t}"));
        }
        req.send().unwrap()
    }
}

impl Drop for Http {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

const INIT: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#;

#[test]
fn http_rejects_missing_and_wrong_tokens() {
    let h = Http::start(&test_config("http-auth"), "s3cret");
    let init: Value = serde_json::from_str(INIT).unwrap();

    assert_eq!(h.post(None, init.clone()).status(), 401);
    assert_eq!(h.post(Some("wrong"), init.clone()).status(), 401);
    assert_eq!(h.post(Some("s3cre"), init.clone()).status(), 401);
    // The root and unknown paths do not reveal anything either.
    assert_eq!(h.client.get(h.url.replace("/mcp", "/")).send().unwrap().status(), 401);
}

#[test]
fn http_with_the_right_token_lists_tools_and_calls_one() {
    let h = Http::start(&test_config("http-ok"), "s3cret");

    let r = h.post(Some("s3cret"), serde_json::from_str(INIT).unwrap());
    assert!(r.status().is_success(), "initialize: {}", r.status());
    let session = r.headers().get("mcp-session-id").cloned();
    let init: Value = r.json().unwrap();
    assert_eq!(init["result"]["serverInfo"]["name"], "vapx", "{init}");

    let call = |body: Value| {
        let mut req = h
            .client
            .post(&h.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("Authorization", "Bearer s3cret")
            .header("MCP-Protocol-Version", "2025-03-26")
            .json(&body);
        if let Some(s) = &session {
            req = req.header("mcp-session-id", s.clone());
        }
        let r = req.send().unwrap();
        assert!(r.status().is_success(), "{}", r.status());
        r.json::<Value>().unwrap()
    };

    let list = call(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    assert_tools_listed(&list["result"]);

    let cams = call(json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                           "params": {"name": "list_cameras", "arguments": {}}}));
    let text = cams["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("\"front\""), "{text}");
}

fn run_to_exit(cmd: &mut Command) -> (bool, String) {
    let out = cmd.stdin(Stdio::null()).output().unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn http_refuses_to_start_without_a_token_or_a_bind_address() {
    let config = test_config("http-refuse");

    let (ok, err) = run_to_exit(
        vapx(&config).args(["mcp", "--transport", "http", "--bind", "127.0.0.1:0"]).env_remove("VAPX_MCP_TOKEN"),
    );
    assert!(!ok, "must not start without a token");
    assert!(err.contains("VAPX_MCP_TOKEN"), "{err}");

    let (ok, err) = run_to_exit(vapx(&config).args(["mcp", "--transport", "http"]).env("VAPX_MCP_TOKEN", "x"));
    assert!(!ok, "must not pick a bind address on its own");
    assert!(err.contains("--bind"), "{err}");
}

// ---- tools against a fake camera ----

/// A camera stand-in that answers by request path (no authentication challenge).
fn fake_camera(routes: Vec<(&'static str, &'static str)>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let routes = routes.clone();
            thread::spawn(move || {
                use std::io::Read;
                let mut buf = vec![0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]).into_owned();
                let path = head.split_whitespace().nth(1).unwrap_or("/").split('?').next().unwrap().to_string();
                let (status, body) = match routes.iter().find(|(p, _)| *p == path) {
                    Some((_, body)) => ("200 OK", *body),
                    None => ("404 Not Found", "not found"),
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

fn config_with_camera(tag: &str, port: u16) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vapx-mcp-{}-{}", std::process::id(), tag));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cameras.yaml");
    std::fs::write(&path, format!("cameras:\n  cam1:\n    host: 127.0.0.1\n    port: {port}\n    user: root\n    pass: pass\n")).unwrap();
    path
}

fn call_text(s: &mut Stdio_, id: i64, tool: &str, args: Value) -> String {
    s.send(json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
                  "params": {"name": tool, "arguments": args}}));
    let r = s.response(id);
    assert_ne!(r["result"]["isError"], json!(true), "{r}");
    r["result"]["content"][0]["text"].as_str().unwrap().to_string()
}

#[test]
fn camera_info_drops_the_vendor_url_and_adds_the_device_url() {
    let port = fake_camera(vec![(
        "/axis-cgi/basicdeviceinfo.cgi",
        r#"{"apiVersion":"1.0","data":{"propertyList":{"ProdNbr":"P1375","Version":"11.11.1","SerialNumber":"ACCC00000000","WebURL":"https://www.axis.com"}}}"#,
    )]);
    let mut s = Stdio_::start(&config_with_camera("info", port));
    s.initialize();

    let info: Value = serde_json::from_str(&call_text(&mut s, 2, "get_camera_info", json!({"camera_id": "cam1"}))).unwrap();
    assert_eq!(info["ProdNbr"], "P1375");
    assert!(info.get("WebURL").is_none());
    assert_eq!(info["DeviceURL"], "http://127.0.0.1");

    let status: Value = serde_json::from_str(&call_text(&mut s, 3, "status_all", json!({}))).unwrap();
    assert_eq!(status[0]["online"], true, "{status}");
    assert_eq!(status[0]["model"], "P1375");
    assert_eq!(status[0]["firmware"], "11.11.1");
}

#[test]
fn system_log_can_be_cut_to_the_last_lines() {
    let port = fake_camera(vec![("/axis-cgi/admin/systemlog.cgi", "one\ntwo\nthree\nfour")]);
    let mut s = Stdio_::start(&config_with_camera("syslog", port));
    s.initialize();

    assert_eq!(call_text(&mut s, 2, "get_system_log", json!({"camera_id": "cam1", "lines": 2})), "three\nfour");
    assert_eq!(call_text(&mut s, 3, "get_system_log", json!({"camera_id": "cam1"})), "one\ntwo\nthree\nfour");
}
