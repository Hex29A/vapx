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

/// The legacy connection list: one line per active client connection. The
/// lines are passed on as the camera wrote them, since the layout differs
/// between firmware generations.
fn get_stream_status_from_connection_list(client: &VapixClient) -> anyhow::Result<Value> {
    let text = client.get_text("/axis-cgi/admin/connection_list.cgi", &[("action", "get")])?;
    let connections = parse_connection_list(&text)?;
    Ok(json!({
        "source": "connection_list_cgi",
        "data": {
            "source": "connection_list_cgi",
            "connections": connections,
            "note": "Active client connections from the legacy connection_list.cgi, as the camera lists them. An empty list means no client is connected.",
        },
    }))
}

/// Non-empty text lines of a `connection_list.cgi` answer. An error page or an
/// `# Error:` answer is an error, so the caller falls through to the next source.
fn parse_connection_list(text: &str) -> anyhow::Result<Vec<String>> {
    let trimmed = text.trim_start();
    let lower = trimmed.get(..15).unwrap_or(trimmed).to_ascii_lowercase();
    if trimmed.starts_with("# Error") || trimmed.starts_with("Error") || lower.starts_with("<!doctype") || lower.starts_with("<html") {
        anyhow::bail!("connection_list.cgi did not return a list");
    }
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect())
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
        let srv = camera_without_streamstatus("192.0.2.7 RTSP\n192.0.2.8 HTTP\n");
        let v = get_stream_status(&srv.client()).unwrap();
        assert_eq!(v["source"], "connection_list_cgi");
        assert_eq!(v["data"]["connections"], json!(["192.0.2.7 RTSP", "192.0.2.8 HTTP"]));
        assert!(!srv.requests().iter().any(|r| r.contains("streamstatus.cgi")), "the POST must be skipped");
    }

    #[test]
    fn an_empty_connection_list_means_nobody_is_connected() {
        let srv = camera_without_streamstatus("\n");
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
