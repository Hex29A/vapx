//! Stream status retrieval.
//!
//! Tries (in order):
//! 1. `POST /axis-cgi/streamstatus.cgi` with method `getAllStreams` (the
//!    Stream status API; administrator access, RTSP streams only). Skipped when
//!    API discovery answers and does not list `streamstatus` (AXIS OS 10.12).
//! 2. `GET /axis-cgi/admin/connection_list.cgi?action=get` — the legacy list
//!    of active client connections, which predates API discovery.
//! 3. `param.cgi root.Image.I0.Stream.*` — labelled fallback that returns
//!    *configuration* parameters with an explanatory note. A value of `0`
//!    here means "unlimited", **not** "no active streams".

use serde_json::{json, Value};
use tracing::debug;

use crate::vapix::client::VapixClient;
use crate::vapix::discover;

/// Retrieve stream status as structured JSON.
///
/// The returned `Value` always contains a `source` field describing where the
/// data came from (`"streamstatus_cgi"`, `"connection_list_cgi"` or
/// `"param_cgi_fallback"`).
pub fn get_stream_status(client: &VapixClient) -> anyhow::Result<Value> {
    if discover::api_listed(client, "streamstatus") == Some(false) {
        debug!("streamstatus is not in API discovery, skipping streamstatus.cgi");
    } else {
        let body = json!({
            "apiVersion": "1.0",
            "method": "getAllStreams",
        });
        match client.post_json("/axis-cgi/streamstatus.cgi", &body) {
            Ok(mut resp) => {
                debug!("streamstatus.cgi getAllStreams succeeded");
                if let Some(obj) = resp.as_object_mut() {
                    obj.insert("source".into(), json!("streamstatus_cgi"));
                }
                return Ok(resp);
            }
            Err(e) => debug!("streamstatus.cgi getAllStreams failed: {}", e),
        }
    }
    match get_stream_status_from_connection_list(client) {
        Ok(v) => return Ok(v),
        Err(e) => debug!("connection_list.cgi failed: {}", e),
    }
    get_stream_status_from_params(client)
}

/// The legacy connection list: one row per active client connection.
fn get_stream_status_from_connection_list(client: &VapixClient) -> anyhow::Result<Value> {
    let text = client.get_text("/axis-cgi/admin/connection_list.cgi", &[("action", "get")])?;
    let connections = parse_connection_list(&text)?;
    Ok(json!({
        "source": "connection_list_cgi",
        "data": {
            "source": "connection_list_cgi",
            "connections": connections,
            "note": "Active client connections from the legacy connection_list.cgi. An empty list means no client is connected. The request that asked for this list is itself a connection to the camera's web server.",
        },
    }))
}

/// The rows of a `connection_list.cgi` answer, as objects with `ip`,
/// `protocol`, `service`, `port`, `pid` and `process`. The camera prints a
/// header row (`IP PROTOCOL SERVICE(PORT) PID/PROCESS`) and then one row per
/// connection; the header is left out. A row that does not fit that layout is
/// kept as `{"raw": line}` so nothing is silently dropped. An error page or an
/// `# Error:` answer is an error, so the caller falls through to the next source.
fn parse_connection_list(text: &str) -> anyhow::Result<Vec<Value>> {
    let trimmed = text.trim_start();
    let lower = trimmed.get(..15).unwrap_or(trimmed).to_ascii_lowercase();
    if trimmed.starts_with("# Error") || trimmed.starts_with("Error") || lower.starts_with("<!doctype") || lower.starts_with("<html") {
        anyhow::bail!("connection_list.cgi did not return a list");
    }
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !is_header_row(l))
        .map(parse_connection_row)
        .collect())
}

fn is_header_row(line: &str) -> bool {
    let mut cols = line.split_whitespace();
    cols.next().is_some_and(|c| c.eq_ignore_ascii_case("ip")) && cols.next().is_some_and(|c| c.eq_ignore_ascii_case("protocol"))
}

fn parse_connection_row(line: &str) -> Value {
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.len() < 3 {
        return json!({"raw": line});
    }
    // "http(80)" -> service "http", port 80; "http" alone has no port.
    let (service, port) = match cols[2].split_once('(') {
        Some((name, rest)) => (name, rest.trim_end_matches(')').parse::<u16>().ok()),
        None => (cols[2], None),
    };
    let mut row = json!({
        "ip": cols[0],
        "protocol": cols[1],
        "service": service,
        "port": port,
    });
    // "3404/httpd" -> pid 3404, process "httpd"; "-" means the camera did not say.
    if let Some(last) = cols.get(3) {
        if let Some((pid, process)) = last.split_once('/') {
            row["pid"] = pid.parse::<u32>().map(Value::from).unwrap_or(Value::Null);
            row["process"] = json!(process);
        } else if *last != "-" {
            row["process"] = json!(last);
        }
    }
    row
}

/// Read stream configuration parameters via param.cgi and label them clearly.
///
/// These are *configuration* values, not live statistics. A value of `0` for
/// `duration`, `fps`, or `nbrOfFrames` means "unlimited", **not** "no active
/// streams". This is communicated via a `note` field in the response.
fn get_stream_status_from_params(client: &VapixClient) -> anyhow::Result<Value> {
    let text = client.get_text(
        "/axis-cgi/param.cgi",
        &[("action", "list"), ("group", "root.Image.I0.Stream")],
    )?;
    if text.starts_with("# Error:") {
        anyhow::bail!("Stream status not available on this camera: {}. Use 'vapx discover' to check supported APIs.", text.trim());
    }

    let mut cfg = serde_json::Map::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let key = k
                .strip_prefix("root.Image.I0.Stream.")
                .unwrap_or(k)
                .to_string();
            cfg.insert(key, json!(v));
        }
    }

    Ok(json!({
        "data": {
            "streamConfig": cfg,
            "source": "param_cgi_fallback",
            "note": "Neither streamstatus.cgi (getAllStreams) nor connection_list.cgi answered on this camera. Showing stream *configuration* parameters from param.cgi — a value of 0 means 'unlimited', not 'no active streams'.",
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vapix::testserver::{Reply, TestServer};

    #[test]
    fn live_streams_come_from_get_all_streams() {
        let srv = TestServer::start(|req| {
            if req.contains(r#""method":"getAllStreams""#) {
                Reply::Json(200, r#"{"apiVersion":"1.0","method":"getAllStreams","data":{"streams":[{"id":11,"state":"playing"}]}}"#)
            } else {
                Reply::Json(400, r#"{"error":{"code":2102,"message":"Method not supported"}}"#)
            }
        });
        let v = get_stream_status(&srv.client()).unwrap();
        assert_eq!(v["source"], "streamstatus_cgi");
        assert_eq!(v["data"]["streams"][0]["id"], 11);
    }

    #[test]
    fn falls_back_to_configuration_when_the_cgi_refuses() {
        let srv = TestServer::start(|req| {
            if req.contains("streamstatus.cgi") {
                Reply::Json(400, r#"{"error":{"code":2102,"message":"Method not supported"}}"#)
            } else if req.contains("connection_list.cgi") {
                Reply::Text(404, "Not Found")
            } else {
                Reply::Text(200, "root.Image.I0.Stream.Duration=0\n")
            }
        });
        let v = get_stream_status(&srv.client()).unwrap();
        assert_eq!(v["data"]["source"], "param_cgi_fallback");
        assert_eq!(v["data"]["streamConfig"]["Duration"], "0");
    }

    /// connection_list.cgi as AXIS OS 10.12 prints it (addresses replaced).
    const LIST_10_12: &str = "IP                                      PROTOCOL  SERVICE(PORT)          PID/PROCESS\n192.0.2.7                               tcp       http(80)               3404/httpd\n192.0.2.8                               tcp       rtsp(554)              3411/rtspd\n";

    #[test]
    fn a_row_outside_the_layout_is_kept_as_text() {
        let rows = parse_connection_list("IP PROTOCOL SERVICE(PORT) PID/PROCESS\nodd\n192.0.2.9 udp ntp -\n").unwrap();
        assert_eq!(rows[0], json!({"raw": "odd"}));
        assert_eq!(rows[1], json!({"ip": "192.0.2.9", "protocol": "udp", "service": "ntp", "port": null}));
    }

    /// AXIS OS 10.12: discovery answers but does not list streamstatus.
    fn camera_without_streamstatus(list: &'static str) -> TestServer {
        TestServer::start(move |req| {
            if req.contains("apidiscovery.cgi") {
                Reply::Json(200, r#"{"apiVersion":"1.0","data":{"apiList":[{"id":"mqtt-client","version":"1.5"}]}}"#)
            } else if req.contains("connection_list.cgi") {
                Reply::Text(200, list)
            } else if req.contains("streamstatus.cgi") {
                Reply::Json(200, r#"{"data":{"streams":[]}}"#)
            } else {
                Reply::Text(200, "root.Image.I0.Stream.Duration=0\n")
            }
        })
    }

    #[test]
    fn unlisted_streamstatus_is_skipped_and_the_connection_list_is_used() {
        let srv = camera_without_streamstatus(LIST_10_12);
        let v = get_stream_status(&srv.client()).unwrap();
        assert_eq!(v["source"], "connection_list_cgi");
        assert_eq!(
            v["data"]["connections"],
            json!([
                {"ip": "192.0.2.7", "protocol": "tcp", "service": "http", "port": 80, "pid": 3404, "process": "httpd"},
                {"ip": "192.0.2.8", "protocol": "tcp", "service": "rtsp", "port": 554, "pid": 3411, "process": "rtspd"},
            ])
        );
        assert!(!srv.requests().iter().any(|r| r.contains("streamstatus.cgi")), "the POST must be skipped");
    }

    #[test]
    fn an_empty_connection_list_means_nobody_is_connected() {
        let srv = camera_without_streamstatus("IP                                      PROTOCOL  SERVICE(PORT)          PID/PROCESS\n");
        let v = get_stream_status(&srv.client()).unwrap();
        assert_eq!(v["source"], "connection_list_cgi");
        assert_eq!(v["data"]["connections"], json!([]));
    }

    #[test]
    fn an_error_answer_from_the_connection_list_falls_back_to_configuration() {
        let srv = camera_without_streamstatus("# Error: not found\n");
        let v = get_stream_status(&srv.client()).unwrap();
        assert_eq!(v["data"]["source"], "param_cgi_fallback");
    }
}
