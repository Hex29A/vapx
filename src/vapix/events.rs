use std::time::{Duration, Instant};

use anyhow::Context;
use serde_json::{json, Value};
use tracing::debug;
use tungstenite::Message;

use crate::config::credentials::Credentials;
use crate::vapix::client::VapixClient;
use crate::vapix::ws::{build_ws_url, connect_ws, get_ws_session, set_read_timeout, WsSocket};

/// Connect to the event WebSocket and stream events.
/// Calls `on_event` for each received event notification.
/// Runs until the connection is closed or `on_event` returns false.
pub fn stream_events(
    client: &VapixClient,
    creds: &Credentials,
    host: &str,
    topic_filter: Option<&str>,
    on_event: impl FnMut(&Value) -> bool,
) -> anyhow::Result<()> {
    stream_events_until(client, creds, host, topic_filter, None, on_event)
}

/// Collect the event notifications that arrive within `seconds`, at most
/// `max_events` of them. Returns the events and whether the limit cut the
/// listening short.
pub fn poll_events(
    client: &VapixClient,
    creds: &Credentials,
    host: &str,
    topic_filter: Option<&str>,
    seconds: f64,
    max_events: usize,
) -> anyhow::Result<(Vec<Value>, bool)> {
    let deadline = Instant::now() + Duration::from_secs_f64(seconds);
    let mut events = Vec::new();
    let mut limited = false;
    stream_events_until(client, creds, host, topic_filter, Some(deadline), |msg| {
        if msg.get("method").and_then(Value::as_str) == Some("events:notify") {
            if events.len() >= max_events {
                limited = true;
                return false;
            }
            events.push(msg.pointer("/params/notification").cloned().unwrap_or(Value::Null));
        }
        true
    })?;
    Ok((events, limited))
}

/// Like [`stream_events`], but stops quietly once `deadline` has passed.
pub fn stream_events_until(
    client: &VapixClient,
    creds: &Credentials,
    host: &str,
    topic_filter: Option<&str>,
    deadline: Option<Instant>,
    mut on_event: impl FnMut(&Value) -> bool,
) -> anyhow::Result<()> {
    let session_token = get_ws_session(client)?;

    let ws_url = build_ws_url(creds, host, &session_token, "events");
    // The URL carries the session token: log the endpoint only.
    debug!("Connecting WebSocket to {}:{}", host, creds.port);

    let mut socket = connect_ws(creds, &ws_url)?;

    debug!("WebSocket connected, sending configure");

    // Build event filter
    let filter = if let Some(topic) = topic_filter {
        json!([{"topicFilter": topic}])
    } else {
        // Subscribe to all events with a broad wildcard
        json!([{"topicFilter": "onvif:Device//."}])
    };

    let configure = json!({
        "apiVersion": "1.0",
        "method": "events:configure",
        "params": {
            "eventFilterList": filter
        }
    });

    socket
        .send(Message::Text(serde_json::to_string(&configure)?.into()))
        .context("Failed to send configure")?;

    // Read the configure response
    limit_read_time(&mut socket, deadline);
    let msg = socket.read().context("Failed to read configure response")?;
    if let Message::Text(text) = &msg {
        let resp: Value = serde_json::from_str(text)?;
        if resp.get("error").is_some() {
            let code = resp.pointer("/error/code").and_then(|c| c.as_i64()).unwrap_or(0);
            let message = resp.pointer("/error/message").and_then(|m| m.as_str()).unwrap_or("Unknown");
            anyhow::bail!("Event configure error {}: {}", code, message);
        }
        debug!("Event stream configured");
    }

    // Stream events
    loop {
        if deadline.is_some_and(|d| Instant::now() >= d) {
            break;
        }
        limit_read_time(&mut socket, deadline);
        match socket.read() {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<Value>(&text) {
                    Ok(event) => {
                        if !on_event(&event) {
                            break;
                        }
                    }
                    Err(e) => {
                        debug!("Non-JSON message: {} ({})", text, e);
                    }
                }
            }
            Ok(Message::Close(_)) => {
                debug!("WebSocket closed by server");
                break;
            }
            Ok(Message::Ping(data)) => {
                let _ = socket.send(Message::Pong(data));
            }
            Ok(_) => {} // Binary, Pong, Frame — ignore
            Err(tungstenite::Error::Io(e))
                if deadline.is_some()
                    && matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) =>
            {
                // The read timed out; the loop ends if the deadline has passed.
            }
            Err(e) => {
                anyhow::bail!("WebSocket error: {}", e);
            }
        }
    }

    Ok(())
}

/// Make blocking reads return when `deadline` arrives, so a quiet camera does
/// not hold the connection past it.
fn limit_read_time(socket: &mut WsSocket, deadline: Option<Instant>) {
    let Some(deadline) = deadline else { return };
    let left = deadline.saturating_duration_since(Instant::now()).max(Duration::from_millis(50));
    set_read_timeout(socket, Some(left));
}
