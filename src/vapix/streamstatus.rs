//! Stream status retrieval.
//!
//! Tries (in order):
//! 1. `POST /axis-cgi/streamstatus.cgi` over HTTP — works on some cameras.
//! 2. `/vapix/ws-data-stream?sources=streamstatus` over WebSocket — the
//!    transport required by newer firmwares.
//! 3. `param.cgi root.Image.I0.Stream.*` — labelled fallback that returns
//!    *configuration* parameters with an explanatory note. A value of `0`
//!    here means "unlimited", **not** "no active streams".

use std::time::Duration;

use serde_json::{json, Value};
use tracing::debug;
use tungstenite::http::Uri;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, Message};

use crate::config::credentials::Credentials;
use crate::vapix::client::VapixClient;
use crate::vapix::ws::{build_ws_url, get_ws_session};

/// Retrieve stream status as structured JSON.
///
/// The returned `Value` always contains a `source` field describing where the
/// data came from (`"streamstatus_cgi"`, `"websocket"`, or
/// `"param_cgi_fallback"`).
pub fn get_stream_status(
    client: &VapixClient,
    creds: &Credentials,
    host: &str,
) -> anyhow::Result<Value> {
    // 1. Try HTTP streamstatus.cgi
    let body = json!({
        "apiVersion": "1.0",
        "method": "getStreamStatus",
    });
    match client.post_json("/axis-cgi/streamstatus.cgi", &body) {
        Ok(mut resp) => {
            debug!("streamstatus.cgi (HTTP) succeeded");
            if let Some(obj) = resp.as_object_mut() {
                obj.insert("source".into(), json!("streamstatus_cgi"));
            }
            return Ok(resp);
        }
        Err(e) => debug!("streamstatus.cgi (HTTP) failed: {}", e),
    }

    // 2. Try WebSocket transport
    match query_streamstatus_ws(client, creds, host) {
        Ok(mut v) => {
            debug!("streamstatus over WebSocket succeeded");
            if let Some(obj) = v.as_object_mut() {
                obj.insert("source".into(), json!("websocket"));
            }
            return Ok(v);
        }
        Err(e) => debug!("streamstatus over WebSocket failed: {}", e),
    }

    // 3. Fallback: labelled param.cgi response
    get_stream_status_from_params(client)
}

/// Query streamstatus over the VAPIX WebSocket data-stream API.
///
/// Connects with `sources=streamstatus`, sends a configure message, reads a
/// single response (subject to a short read timeout), then closes.
fn query_streamstatus_ws(
    client: &VapixClient,
    creds: &Credentials,
    host: &str,
) -> anyhow::Result<Value> {
    let token = get_ws_session(client)?;
    let url = build_ws_url(creds, host, &token, "streamstatus");
    debug!("Connecting WS for streamstatus: {}", url);

    let uri: Uri = url.parse()?;
    let (mut socket, _resp) = connect(uri)?;

    // Set a 3s read timeout on the underlying stream so we don't hang
    // indefinitely if the camera doesn't reply.
    if let MaybeTlsStream::Plain(ref s) = socket.get_ref() {
        let _ = s.set_read_timeout(Some(Duration::from_secs(3)));
    }

    let configure = json!({
        "apiVersion": "1.0",
        "method": "streamstatus:configure",
        "params": {},
    });
    socket.send(Message::Text(serde_json::to_string(&configure)?.into()))?;

    // Read until we get a data payload, or the timeout fires.
    let mut last_data: Option<Value> = None;
    for _ in 0..4 {
        match socket.read() {
            Ok(Message::Text(text)) => {
                let v: Value = serde_json::from_str(&text)?;
                if let Some(err) = v.get("error") {
                    anyhow::bail!("WS streamstatus error: {}", err);
                }
                // The first reply is typically the configure ACK. The payload
                // we actually want is the next message containing stream data.
                if v.get("data").is_some() || v.get("streams").is_some() {
                    last_data = Some(v);
                    break;
                }
                last_data = Some(v);
            }
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(p)) => {
                let _ = socket.send(Message::Pong(p));
            }
            Ok(_) => {}
            Err(e) => {
                debug!("WS read ended: {}", e);
                break;
            }
        }
    }
    let _ = socket.close(None);

    let payload =
        last_data.ok_or_else(|| anyhow::anyhow!("No data received from WS streamstatus"))?;

    if let Some(data) = payload.get("data").cloned() {
        Ok(json!({ "data": data }))
    } else {
        Ok(json!({ "data": payload }))
    }
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
            "note": "streamstatus.cgi requires WebSocket transport on this firmware (and the WS attempt also failed). Showing stream *configuration* parameters from param.cgi — a value of 0 means 'unlimited', not 'no active streams'.",
        }
    }))
}
