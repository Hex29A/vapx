//! Stream status retrieval.
//!
//! Tries (in order):
//! 1. `POST /axis-cgi/streamstatus.cgi` with method `getAllStreams` (the
//!    Stream status API; administrator access, RTSP streams only).
//! 2. `param.cgi root.Image.I0.Stream.*` — labelled fallback that returns
//!    *configuration* parameters with an explanatory note. A value of `0`
//!    here means "unlimited", **not** "no active streams".

use serde_json::{json, Value};
use tracing::debug;

use crate::vapix::client::VapixClient;

/// Retrieve stream status as structured JSON.
///
/// The returned `Value` always contains a `source` field describing where the
/// data came from (`"streamstatus_cgi"` or `"param_cgi_fallback"`).
pub fn get_stream_status(client: &VapixClient) -> anyhow::Result<Value> {
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
    get_stream_status_from_params(client)
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
            "note": "streamstatus.cgi (getAllStreams) did not answer on this camera. Showing stream *configuration* parameters from param.cgi — a value of 0 means 'unlimited', not 'no active streams'.",
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
            } else {
                Reply::Text(200, "root.Image.I0.Stream.Duration=0\n")
            }
        });
        let v = get_stream_status(&srv.client()).unwrap();
        assert_eq!(v["data"]["source"], "param_cgi_fallback");
        assert_eq!(v["data"]["streamConfig"]["Duration"], "0");
    }
}
