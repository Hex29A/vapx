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
        Self::start_env(config, &[])
    }

    fn start_env(config: &PathBuf, env: &[(&str, &str)]) -> Self {
        let mut child = vapx(config)
            .arg("mcp")
            .envs(env.iter().copied())
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

type Log = std::sync::Arc<std::sync::Mutex<Vec<String>>>;

/// How a fake camera answers one path.
struct Route(&'static str, &'static str, &'static str); // path, content type, body

fn json_route(path: &'static str, body: &'static str) -> Route {
    Route(path, "application/json", body)
}

/// A camera stand-in. It answers by request path (no authentication
/// challenge), `{"data":{}}` for paths it has no route for, and records each
/// request as `METHOD target body`. With `drop_connections` it records the
/// request and closes the connection without answering, as a camera does
/// while it reboots.
fn camera_stub(routes: Vec<Route>, drop_connections: bool) -> (u16, Log) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let log: Log = Default::default();
    let shared = log.clone();
    let routes = std::sync::Arc::new(routes);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let (routes, log) = (routes.clone(), shared.clone());
            thread::spawn(move || {
                use std::io::Read;
                let mut data = Vec::new();
                let mut buf = [0u8; 4096];
                // Read the head, then as much body as Content-Length announces.
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
                if drop_connections {
                    return;
                }
                let path = target.split('?').next().unwrap();
                let (ctype, reply) = routes
                    .iter()
                    .find(|r| r.0 == path)
                    .map(|r| (r.1, r.2))
                    .unwrap_or(("application/json", r#"{"apiVersion":"1.0","data":{}}"#));
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

fn fake_camera(routes: Vec<Route>) -> (u16, Log) {
    camera_stub(routes, false)
}

fn requests(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
}

fn config_with_camera(tag: &str, port: u16) -> PathBuf {
    config_with_camera_extra(tag, port, "")
}

fn config_with_camera_extra(tag: &str, port: u16, extra: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vapx-mcp-{}-{}", std::process::id(), tag));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cameras.yaml");
    std::fs::write(
        &path,
        format!("cameras:\n  cam1:\n    host: 127.0.0.1\n    port: {port}\n    user: root\n    pass: pass\n{extra}"),
    )
    .unwrap();
    path
}

fn call(s: &mut Stdio_, id: i64, tool: &str, args: Value) -> Value {
    s.send(json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
                  "params": {"name": tool, "arguments": args}}));
    s.response(id)["result"].clone()
}

fn call_text(s: &mut Stdio_, id: i64, tool: &str, args: Value) -> String {
    let r = call(s, id, tool, args);
    assert_ne!(r["isError"], json!(true), "{r}");
    r["content"][0]["text"].as_str().unwrap().to_string()
}

fn error_text(s: &mut Stdio_, id: i64, tool: &str, args: Value) -> String {
    let r = call(s, id, tool, args);
    assert_eq!(r["isError"], json!(true), "{r}");
    r["content"][0]["text"].as_str().unwrap().to_string()
}

#[test]
fn camera_info_drops_the_vendor_url_and_adds_the_device_url() {
    let (port, _) = fake_camera(vec![json_route(
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
    let (port, _) = fake_camera(vec![Route("/axis-cgi/admin/systemlog.cgi", "text/plain", "one\ntwo\nthree\nfour")]);
    let mut s = Stdio_::start(&config_with_camera("syslog", port));
    s.initialize();

    assert_eq!(call_text(&mut s, 2, "get_system_log", json!({"camera_id": "cam1", "lines": 2})), "three\nfour");
    assert_eq!(call_text(&mut s, 3, "get_system_log", json!({"camera_id": "cam1"})), "one\ntwo\nthree\nfour");
}

// ---- write tools (#58) ----

#[test]
fn tools_list_has_the_write_tools_and_marks_the_disruptive_ones() {
    let mut s = Stdio_::start(&test_config("write-list"));
    s.initialize();
    s.send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let r = s.response(2);
    let tools = r["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 93, "the 78 tools of the Python server and the 15 CLI tools");
    let reboot = tools.iter().find(|t| t["name"] == "reboot_camera").unwrap();
    assert_eq!(reboot["annotations"]["destructiveHint"], true, "{reboot}");
    assert_eq!(reboot["annotations"]["readOnlyHint"], false);
    assert!(reboot["description"].as_str().unwrap().contains("Disruptive"));
    let info = tools.iter().find(|t| t["name"] == "get_camera_info").unwrap();
    assert_eq!(info["annotations"]["readOnlyHint"], true);
}

#[test]
fn invalid_arguments_are_rejected_before_any_request_is_made() {
    let (port, log) = fake_camera(vec![]);
    let mut s = Stdio_::start(&config_with_camera("invalid", port));
    s.initialize();

    let e = error_text(&mut s, 2, "ptz_move", json!({"camera_id": "cam1", "pan": 0, "tilt": 0, "zoom": 0}));
    assert!(e.contains("zoom") && e.contains("at least"), "{e}");
    let e = error_text(&mut s, 3, "set_io_port", json!({"camera_id": "cam1", "port": "0", "state": "ajar"}));
    assert!(e.contains("one of"), "{e}");
    let e = error_text(&mut s, 4, "toggle_light", json!({"camera_id": "cam1", "light_id": "led0"}));
    assert!(e.contains("on is required"), "{e}");
    assert!(requests(&log).is_empty(), "no request may reach the camera: {:?}", requests(&log));
}

#[test]
fn ptz_move_sends_the_absolute_position() {
    let (port, log) = fake_camera(vec![]);
    let mut s = Stdio_::start(&config_with_camera("ptz", port));
    s.initialize();

    let text = call_text(&mut s, 2, "ptz_move", json!({"camera_id": "cam1", "pan": 10, "tilt": -5.5, "zoom": 100}));
    assert_eq!(text, "Moved cam1 to pan=10, tilt=-5.5, zoom=100");
    let reqs = requests(&log);
    let req = reqs.iter().find(|r| r.contains("/axis-cgi/com/ptz.cgi")).expect("a PTZ request");
    assert!(req.starts_with("GET ") && req.contains("pan=10") && req.contains("tilt=-5.5") && req.contains("zoom=100"), "{req}");
}

#[test]
fn io_light_timezone_and_siren_calls_reach_the_camera_as_documented() {
    let (port, log) = fake_camera(vec![]);
    let mut s = Stdio_::start(&config_with_camera("calls", port));
    s.initialize();

    call_text(&mut s, 2, "set_io_port", json!({"camera_id": "cam1", "port": "1", "state": "closed"}));
    call_text(&mut s, 3, "toggle_light", json!({"camera_id": "cam1", "light_id": "led0", "on": false}));
    call_text(&mut s, 4, "set_timezone", json!({"camera_id": "cam1", "timezone": "Europe/Stockholm"}));
    call_text(&mut s, 5, "stop_siren", json!({"camera_id": "cam1"}));
    call_text(&mut s, 6, "activate_siren", json!({"camera_id": "cam1", "siren_pattern": "wail", "duration": 3}));

    let all = requests(&log).join("\n");
    for want in [
        "/axis-cgi/io/portmanagement.cgi", r#""method":"setPorts""#, r#""state":"closed""#,
        "/axis-cgi/lightcontrol.cgi", r#""method":"deactivateLight""#, r#""lightID":"led0""#,
        "/axis-cgi/time.cgi", r#""method":"setTimeZone""#, "Europe/Stockholm",
        "/axis-cgi/siren_and_light.cgi", r#""method":"stop""#, r#""method":"start""#,
        r#""pattern":"wail""#, r#""unit":"seconds""#,
    ] {
        assert!(all.contains(want), "missing {want} in:\n{all}");
    }
}

#[test]
fn applications_and_audio_clips_use_the_cameras_own_apis() {
    let (port, log) = fake_camera(vec![Route("/axis-cgi/applications/control.cgi", "text/plain", "OK")]);
    let mut s = Stdio_::start(&config_with_camera("apps", port));
    s.initialize();

    assert_eq!(call_text(&mut s, 2, "restart_application", json!({"camera_id": "cam1", "package_name": "larod"})),
               "Restarted 'larod' on cam1");
    let clip: Value = serde_json::from_str(&call_text(&mut s, 3, "play_audio_clip", json!({"camera_id": "cam1", "name_or_id": "3"}))).unwrap();
    assert_eq!(clip, json!({"clip_id": 3, "status": "playing"}));
    let all = requests(&log).join("\n");
    assert!(all.contains("action=restart") && all.contains("package=larod"), "{all}");
    assert!(all.contains("/axis-cgi/mediaclip.cgi") && all.contains("action=play"), "{all}");
}

#[test]
fn privacy_masks_are_converted_for_rotated_cameras() {
    let (port, log) = fake_camera(vec![Route(
        "/axis-cgi/param.cgi",
        "text/plain",
        "root.Image.I0.Appearance.Resolution=1920x1080",
    ), Route("/axis-cgi/privacymask.cgi", "text/plain", "")]);
    let mut s = Stdio_::start(&config_with_camera_extra("mask", port, "    mask_rotation: 180\n"));
    s.initialize();

    let text = call_text(&mut s, 2, "add_privacy_mask", json!({"camera_id": "cam1", "name": "door", "polygon": "100,100:200,100"}));
    assert!(text.contains("auto-corrected for 180"), "{text}");
    let reqs = requests(&log);
    let req = reqs.iter().find(|r| r.contains("privacymask.cgi")).expect("a mask request");
    // (100,100) and (200,100) on a 180 degree rotated 1920x1080 sensor.
    assert!(req.contains("action=add") && req.contains("name=door"), "{req}");
    assert!(req.contains("1820%2C980%3A1720%2C980") || req.contains("1820,980:1720,980"), "{req}");
}

#[test]
fn reboot_is_sent_once_and_a_dropped_connection_counts_as_sent() {
    let (port, log) = camera_stub(vec![], true);
    let mut s = Stdio_::start(&config_with_camera("reboot", port));
    s.initialize();

    let result: Value = serde_json::from_str(&call_text(&mut s, 2, "reboot_camera", json!({"camera_id": "cam1"}))).unwrap();
    assert_eq!(result["status"], "rebooting", "{result}");
    assert_eq!(result["camera"], "cam1");
    assert!(result["note"].as_str().unwrap().contains("unreachable"));
    assert_eq!(requests(&log).len(), 1, "the reboot request must be sent exactly once: {:?}", requests(&log));
}

#[test]
fn export_writes_into_the_exports_directory_and_nowhere_else() {
    let (port, log) = fake_camera(vec![Route("/axis-cgi/record/export/exportrecording.cgi", "video/x-matroska", "MKVDATA")]);
    let dir = std::env::temp_dir().join(format!("vapx-mcp-exports-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut s = Stdio_::start_env(&config_with_camera("export", port), &[("VAPX_MCP_EXPORTS_DIR", dir.to_str().unwrap())]);
    s.initialize();

    let result: Value = serde_json::from_str(&call_text(
        &mut s, 2, "export_recording", json!({"camera_id": "cam1", "recording_id": "r1", "disk_id": "SD_DISK"}),
    ))
    .unwrap();
    assert_eq!(result["size_bytes"], 7, "{result}");
    assert_eq!(std::fs::read(dir.join("r1.mkv")).unwrap(), b"MKVDATA");

    let before = requests(&log).len();
    let e = error_text(&mut s, 3, "export_recording", json!({
        "camera_id": "cam1", "recording_id": "r1", "disk_id": "SD_DISK", "filename": "../evil.mkv"}));
    assert!(e.contains("plain file name"), "{e}");
    assert_eq!(requests(&log).len(), before, "a rejected filename must not even download");
    assert!(!dir.parent().unwrap().join("evil.mkv").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn snapshot_all_returns_an_image_per_camera() {
    let (port, _) = fake_camera(vec![Route("/axis-cgi/jpg/image.cgi", "image/jpeg", "JPEGBYTES")]);
    let mut s = Stdio_::start(&config_with_camera("snapall", port));
    s.initialize();

    let r = call(&mut s, 2, "snapshot_all", json!({}));
    assert_ne!(r["isError"], json!(true), "{r}");
    assert_eq!(r["content"][0]["text"], "--- cam1 (cam1) ---");
    assert_eq!(r["content"][1]["type"], "image");
    assert_eq!(r["content"][1]["mimeType"], "image/jpeg");
    assert_eq!(r["content"][1]["data"], "SlBFR0JZVEVT"); // base64 of JPEGBYTES
}

// ---- tools for CLI features (#59) ----

/// A config with several cameras, each on its own stub port.
fn config_with_cameras(tag: &str, cams: &[(&str, u16)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vapx-mcp-{}-{}", std::process::id(), tag));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cameras.yaml");
    let mut yaml = String::from("cameras:\n");
    for (name, port) in cams {
        yaml.push_str(&format!("  {name}:\n    host: 127.0.0.1\n    port: {port}\n    user: root\n    pass: pass\n"));
    }
    std::fs::write(&path, yaml).unwrap();
    path
}

const FIRMWARE: &str = "/axis-cgi/firmwaremanagement.cgi";

fn firmware_stub(version: &'static str) -> (u16, Log) {
    camera_stub(vec![], false).tap_routes(version)
}

trait Tap {
    fn tap_routes(self, version: &'static str) -> (u16, Log);
}

impl Tap for (u16, Log) {
    /// Replace the plain stub by one that reports `version` as the active firmware.
    fn tap_routes(self, version: &'static str) -> (u16, Log) {
        let body: &'static str = Box::leak(
            format!(r#"{{"apiVersion":"1.0","data":{{"activeFirmwareVersion":"{version}","firmwareVersion":"12.0.1"}}}}"#).into_boxed_str(),
        );
        camera_stub(vec![json_route(FIRMWARE, body)], false)
    }
}

#[test]
fn firmware_status_and_checks() {
    let (port_a, _) = firmware_stub("11.11.1");
    let (port_b, _) = firmware_stub("12.0.0");
    let mut s = Stdio_::start(&config_with_cameras("fwcheck", &[("a", port_a), ("b", port_b)]));
    s.initialize();

    let status: Value = serde_json::from_str(&call_text(&mut s, 2, "firmware_status", json!({"camera_id": "a"}))).unwrap();
    assert_eq!(status["activeFirmwareVersion"], "11.11.1");

    let ok: Value = serde_json::from_str(&call_text(&mut s, 3, "firmware_check", json!({"camera_id": "a", "expected_version": "11.11.1"}))).unwrap();
    assert_eq!(ok["matches"], true, "{ok}");
    let bad: Value = serde_json::from_str(&call_text(&mut s, 4, "firmware_check", json!({"camera_id": "a", "expected_version": "12.0.0"}))).unwrap();
    assert_eq!(bad["matches"], false);
    let cmp: Value = serde_json::from_str(&call_text(&mut s, 5, "firmware_check", json!({"camera_id": "a", "compare_to": "b"}))).unwrap();
    assert_eq!((cmp["version_a"].as_str(), cmp["version_b"].as_str(), &cmp["matches"]), (Some("11.11.1"), Some("12.0.0"), &json!(false)));
    let e = error_text(&mut s, 6, "firmware_check", json!({"camera_id": "a"}));
    assert!(e.contains("expected_version or compare_to"), "{e}");
}

#[test]
fn firmware_upgrade_needs_confirmation_and_a_bin_file() {
    let (port, log) = firmware_stub("11.0.0");
    let dir = std::env::temp_dir().join(format!("vapx-mcp-fw-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bin = dir.join("axis.bin");
    std::fs::write(&bin, b"FWBYTES").unwrap();
    std::fs::write(dir.join("notes.txt"), b"x").unwrap();
    let mut s = Stdio_::start(&config_with_camera("fwup", port));
    s.initialize();
    let path = bin.to_str().unwrap();

    let e = error_text(&mut s, 2, "firmware_upgrade", json!({"camera_id": "cam1", "file_path": path, "confirm": false}));
    assert!(e.contains("confirm"), "{e}");
    let e = error_text(&mut s, 3, "firmware_upgrade", json!({"camera_id": "cam1", "file_path": path}));
    assert!(e.contains("confirm is required"), "{e}");
    let e = error_text(&mut s, 4, "firmware_upgrade", json!({
        "camera_id": "cam1", "file_path": dir.join("notes.txt").to_str().unwrap(), "confirm": true}));
    assert!(e.contains(".bin"), "{e}");
    let e = error_text(&mut s, 5, "firmware_upgrade", json!({"camera_id": "cam1", "file_path": path, "confirm": true, "auto_commit": true}));
    assert!(e.contains("auto_commit needs wait"), "{e}");
    assert!(requests(&log).is_empty(), "nothing may be sent before the checks pass: {:?}", requests(&log));

    let r: Value = serde_json::from_str(&call_text(&mut s, 6, "firmware_upgrade", json!({"camera_id": "cam1", "file_path": path, "confirm": true}))).unwrap();
    assert_eq!(r["previousVersion"], "11.0.0", "{r}");
    assert_eq!(r["newVersion"], "12.0.1");
    assert_eq!(r["committed"], false);
    let all = requests(&log).join("\n");
    assert!(all.contains("FWBYTES") && all.contains(r#""method":"upgrade""#), "the file must be uploaded: {all}");
    assert!(!all.contains("factoryDefault"), "a factory default must never be requested");
    let _ = std::fs::remove_dir_all(&dir);
}

const PARAMS: &str = "root.Brand.Brand=AXIS\nroot.MQTT.Password=hunter2\nroot.SNMP.V2.ReadCommunity=public\n";

#[test]
fn param_get_redacts_secrets() {
    let (port, log) = fake_camera(vec![Route("/axis-cgi/param.cgi", "text/plain", PARAMS)]);
    let mut s = Stdio_::start(&config_with_camera("pget", port));
    s.initialize();

    let text = call_text(&mut s, 2, "param_get", json!({"camera_id": "cam1", "param": "root"}));
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["root.Brand.Brand"], "AXIS");
    assert_eq!(v["root.MQTT.Password"], "[redacted]");
    assert_eq!(v["root.SNMP.V2.ReadCommunity"], "[redacted]");
    assert!(!text.contains("hunter2") && !text.contains("public"));
    assert!(requests(&log)[0].contains("group=root"));
    let e = error_text(&mut s, 3, "param_get", json!({"camera_id": "cam1", "param": "Brand"}));
    assert!(e.contains("start with root."), "{e}");
}

#[test]
fn param_set_takes_explicit_unprotected_keys_only() {
    let (port, log) = fake_camera(vec![Route("/axis-cgi/param.cgi", "text/plain", "OK")]);
    let mut s = Stdio_::start(&config_with_camera("pset", port));
    s.initialize();

    for (i, key) in ["root.Network.SSH.Enabled", "root.Users.U0.Name", "root.Image.*", "root.Image.", "Image.I0.Rotation"].iter().enumerate() {
        let e = error_text(&mut s, 2 + i as i64, "param_set", json!({"camera_id": "cam1", "assignments": {*key: "x"}}));
        assert!(e.contains("protected") || e.contains("explicit"), "{key}: {e}");
    }
    let e = error_text(&mut s, 10, "param_set", json!({"camera_id": "cam1", "assignments": {}}));
    assert!(e.contains("between 1 and"), "{e}");
    let e = error_text(&mut s, 11, "param_set", json!({"camera_id": "cam1", "assignments": {"root.Image.I0.Appearance.Rotation": {"a": 1}}}));
    assert!(e.contains("string, number or boolean"), "{e}");
    assert!(requests(&log).is_empty(), "refused keys must not reach the camera: {:?}", requests(&log));

    let r: Value = serde_json::from_str(&call_text(&mut s, 12, "param_set", json!({
        "camera_id": "cam1", "assignments": {"root.Image.I0.Appearance.Rotation": 180, "root.Image.I0.Appearance.Mirror": true}}))).unwrap();
    assert_eq!(r["updated"], 2, "{r}");
    let req = requests(&log).join("\n");
    assert!(req.contains("action=update") && req.contains("root.Image.I0.Appearance.Rotation=180") && req.contains("Mirror=yes"), "{req}");
}

#[test]
fn imaging_settings_can_be_read_and_only_existing_ones_set() {
    let (port, log) = fake_camera(vec![Route("/axis-cgi/param.cgi", "text/plain", "root.ImageSource.I0.Sensor.WDR=off\nroot.ImageSource.I0.Sensor.Brightness=50\n")]);
    let mut s = Stdio_::start(&config_with_camera("imaging", port));
    s.initialize();

    let v: Value = serde_json::from_str(&call_text(&mut s, 2, "imaging_get", json!({"camera_id": "cam1"}))).unwrap();
    assert_eq!(v["root.ImageSource.I0.Sensor.WDR"], "off");

    let e = error_text(&mut s, 3, "imaging_set", json!({"camera_id": "cam1", "settings": {"Nonsense": 1}}));
    assert!(e.contains("not an image sensor setting"), "{e}");
    let e = error_text(&mut s, 4, "imaging_set", json!({"camera_id": "cam1", "settings": {"root.Network.SSH.Enabled": "yes"}}));
    assert!(e.contains("not an image sensor setting"), "{e}");
    assert!(requests(&log).iter().all(|r| !r.contains("action=update")), "{:?}", requests(&log));

    call_text(&mut s, 5, "imaging_set", json!({"camera_id": "cam1", "settings": {"WDR": "on"}}));
    assert!(requests(&log).iter().any(|r| r.contains("action=update") && r.contains("root.ImageSource.I0.Sensor.WDR=on")), "{:?}", requests(&log));
}

fn healthy_camera() -> (u16, Log) {
    fake_camera(vec![
        json_route("/axis-cgi/basicdeviceinfo.cgi", r#"{"data":{"propertyList":{"Model":"P1375","SerialNumber":"S1"}}}"#),
        json_route(FIRMWARE, r#"{"data":{"activeFirmwareVersion":"11.11.1"}}"#),
        Route("/axis-cgi/param.cgi", "text/plain", "root.Network.UPnP.Enabled=yes\nroot.Network.SSH.Enabled=yes\n"),
    ])
}

/// A port nothing listens on.
fn dead_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

#[test]
fn fleet_health_reports_each_camera_and_a_summary() {
    let (up, _) = healthy_camera();
    let mut s = Stdio_::start(&config_with_cameras("fleet", &[("up", up), ("down", dead_port())]));
    s.initialize();

    let v: Value = serde_json::from_str(&call_text(&mut s, 2, "fleet_health", json!({}))).unwrap();
    assert_eq!(v["summary"], json!({"total": 2, "healthy": 1, "degraded": 0, "unreachable": 1}), "{v}");
    let cams = v["cameras"].as_array().unwrap();
    let up = cams.iter().find(|c| c["camera"] == "up").unwrap();
    assert_eq!((up["status"].as_str(), up["model"].as_str(), up["firmware"].as_str()), (Some("healthy"), Some("P1375"), Some("11.11.1")));
    assert_eq!(cams.iter().find(|c| c["camera"] == "down").unwrap()["status"], "unreachable");

    let one: Value = serde_json::from_str(&call_text(&mut s, 3, "fleet_health", json!({"targets": "up"}))).unwrap();
    assert_eq!(one["summary"]["total"], 1);
    let e = error_text(&mut s, 4, "fleet_health", json!({"targets": "nope"}));
    assert!(e.contains("nope"), "{e}");
}

#[test]
fn security_audit_covers_one_camera_or_all() {
    let (port, _) = healthy_camera();
    let mut s = Stdio_::start(&config_with_cameras("audit", &[("front", port)]));
    s.initialize();

    let one: Value = serde_json::from_str(&call_text(&mut s, 2, "security_audit", json!({"camera_id": "front"}))).unwrap();
    let codes: Vec<&str> = one["findings"].as_array().unwrap().iter().map(|f| f["code"].as_str().unwrap()).collect();
    for want in ["UNENCRYPTED_HTTP", "UPNP_ENABLED", "SSH_ENABLED", "DEFAULT_USER"] {
        assert!(codes.contains(&want), "missing {want} in {codes:?}");
    }
    assert_eq!(one["model"], "P1375");

    let all: Value = serde_json::from_str(&call_text(&mut s, 3, "security_audit", json!({}))).unwrap();
    assert_eq!(all["cameras"].as_array().unwrap().len(), 1);
    assert_eq!(all["cameras"][0]["host"], "front");
}

#[test]
fn backup_keeps_secrets_in_a_private_file_and_returns_only_the_count() {
    let (port, _) = fake_camera(vec![Route("/axis-cgi/param.cgi", "text/plain", PARAMS)]);
    let dir = std::env::temp_dir().join(format!("vapx-mcp-backup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut s = Stdio_::start_env(&config_with_camera("backup", port), &[("VAPX_MCP_EXPORTS_DIR", dir.to_str().unwrap())]);
    s.initialize();

    let text = call_text(&mut s, 2, "backup_params", json!({"camera_id": "cam1"}));
    assert!(!text.contains("hunter2"), "the reply must not carry the parameters: {text}");
    let r: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(r["parameters"], 3);
    let file = dir.join("cam1-params.json");
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(saved["root.MQTT.Password"], "hunter2");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
    }
    let e = error_text(&mut s, 3, "backup_params", json!({"camera_id": "cam1", "filename": "../x.json"}));
    assert!(e.contains("plain file name"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn diff_params_lists_differences_and_hides_secrets() {
    let (a, _) = fake_camera(vec![Route("/axis-cgi/param.cgi", "text/plain", "root.A=1\nroot.MQTT.Password=one\nroot.Same=x\n")]);
    let (b, _) = fake_camera(vec![Route("/axis-cgi/param.cgi", "text/plain", "root.A=2\nroot.MQTT.Password=two\nroot.Same=x\nroot.New=y\n")]);
    let mut s = Stdio_::start(&config_with_cameras("diff", &[("one", a), ("two", b)]));
    s.initialize();

    let text = call_text(&mut s, 2, "diff_params", json!({"camera_a": "one", "camera_b": "two"}));
    assert!(!text.contains("\"one\": \"one\"") && !text.contains("hunter"), "{text}");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["total_diffs"], 3, "{v}");
    let diffs = v["diffs"].as_array().unwrap();
    let get = |p: &str| diffs.iter().find(|d| d["param"] == p).unwrap_or_else(|| panic!("{p}: {v}")).clone();
    assert_eq!((get("root.A")["one"].as_str(), get("root.A")["two"].as_str()), (Some("1"), Some("2")));
    assert_eq!(get("root.MQTT.Password")["one"], "[redacted]");
    assert_eq!(get("root.MQTT.Password")["two"], "[redacted]");
    assert_eq!(get("root.New")["change"], "only_in_second");
}

#[test]
fn certificates_stream_urls_selftest_and_zipstream() {
    let (port, log) = fake_camera(vec![
        json_route("/axis-cgi/certificate/certificate.cgi", r#"{"data":{"certs":[{"id":"c1"}]}}"#),
        json_route("/axis-cgi/deviceselftest.cgi", r#"{"data":{"result":"pass"}}"#),
        Route("/axis-cgi/zipstream/listprofiles.cgi", "text/xml",
              r#"<Zipstream><MaxLevel>100</MaxLevel><Profile name="classic"><Level>10</Level></Profile></Zipstream>"#),
    ]);
    let mut s = Stdio_::start(&config_with_camera("misc", port));
    s.initialize();

    let certs: Value = serde_json::from_str(&call_text(&mut s, 2, "cert_list", json!({"camera_id": "cam1"}))).unwrap();
    assert_eq!(certs["certs"][0]["id"], "c1");
    let st: Value = serde_json::from_str(&call_text(&mut s, 3, "selftest", json!({"camera_id": "cam1"}))).unwrap();
    assert_eq!(st["result"], "pass");
    let zs: Value = serde_json::from_str(&call_text(&mut s, 4, "zipstream_status", json!({"camera_id": "cam1"}))).unwrap();
    assert_eq!(zs["maxLevel"], 100);
    assert_eq!(zs["profiles"][0]["name"], "classic");

    let before = requests(&log).len();
    let text = call_text(&mut s, 5, "stream_urls", json!({"camera_id": "cam1", "resolution": "1280x720", "fps": 10}));
    assert_eq!(requests(&log).len(), before, "building URLs must not call the camera");
    let u: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(u["rtsp"], "rtsp://127.0.0.1:554/axis-media/media.amp?videocodec=h264&camera=1&resolution=1280x720&fps=10");
    assert!(u["mjpeg"].as_str().unwrap().starts_with("http://127.0.0.1:"), "{u}");
    assert!(!text.contains("root:") && !text.contains("pass@"), "no credentials in URLs: {text}");
    let e = error_text(&mut s, 6, "stream_urls", json!({"camera_id": "cam1", "resolution": "big&evil"}));
    assert!(e.contains("1920x1080"), "{e}");
}

// ---- #65: nothing but JSON-RPC on stdout ----

/// Log lines (retries, HTTP 500, unreachable cameras) must not reach stdout,
/// which is the protocol channel in stdio mode, and must carry no colour codes
/// when stderr is not a terminal.
#[test]
fn stdio_stdout_carries_only_json_rpc_even_when_cameras_fail() {
    let (broken, _) = camera_stub(vec![], false); // answers 200 {} for everything
    let (erroring, _) = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                thread::spawn(move || {
                    use std::io::Read;
                    let mut buf = [0u8; 4096];
                    let _ = stream.read(&mut buf);
                    let body = "Internal server error - AXIS";
                    let _ = write!(stream, "HTTP/1.1 500 Internal Server Error\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                });
            }
        });
        (port, ())
    };
    let config = config_with_cameras("stdout-clean", &[("fine", broken), ("err500", erroring), ("gone", dead_port())]);

    let mut child = vapx(&config).args(["-v", "mcp"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for msg in [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "status_all", "arguments": {}}}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "get_motion_config", "arguments": {"camera_id": "err500"}}}),
    ] {
        writeln!(stdin, "{msg}").unwrap();
    }
    // Read stdout line by line until both calls are answered, then close stdin to end the session.
    let stdout_pipe = child.stdout.take().unwrap();
    let (tx, rx) = channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout_pipe).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut ids = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !(ids.contains(&json!(2)) && ids.contains(&json!(3))) {
        let left = deadline.saturating_duration_since(Instant::now());
        let line = rx.recv_timeout(left).expect("both calls must be answered");
        let v: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("non-JSON on stdout ({e}): {line:?}"));
        assert_eq!(v["jsonrpc"], "2.0", "{line}");
        ids.push(v["id"].clone());
    }
    drop(stdin);
    for line in rx.iter() {
        serde_json::from_str::<Value>(&line).unwrap_or_else(|e| panic!("non-JSON on stdout ({e}): {line:?}"));
    }
    let out = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Retry"), "the retries should be logged on stderr: {stderr}");
    assert!(!stderr.contains('\u{1b}'), "no colour codes when stderr is not a terminal: {stderr:?}");
}
