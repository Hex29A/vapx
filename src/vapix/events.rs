use anyhow::Context;
use serde_json::{json, Value};
use tracing::debug;
use tungstenite::http::Uri;
use tungstenite::{connect, Message};

use crate::config::credentials::Credentials;
use crate::vapix::client::VapixClient;
use crate::vapix::ws::{build_ws_url, get_ws_session};

/// Connect to the event WebSocket and stream events.
/// Calls `on_event` for each received event notification.
/// Runs until the connection is closed or `on_event` returns false.
pub fn stream_events(
    client: &VapixClient,
    creds: &Credentials,
    host: &str,
    topic_filter: Option<&str>,
    mut on_event: impl FnMut(&Value) -> bool,
) -> anyhow::Result<()> {
    let session_token = get_ws_session(client)?;

    let ws_url = build_ws_url(creds, host, &session_token, "events");
    debug!("Connecting WebSocket: {}", ws_url);

    let uri: Uri = ws_url.parse().context("Invalid WebSocket URL")?;
    let (mut socket, _response) = connect(uri).context("WebSocket connection failed")?;

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
            Err(e) => {
                anyhow::bail!("WebSocket error: {}", e);
            }
        }
    }

    Ok(())
}
