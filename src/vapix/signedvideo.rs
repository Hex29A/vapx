use crate::vapix::client::VapixClient;
use serde_json::{json, Value};
use tracing::debug;

/// Post to signedvideo.cgi, returning Ok(resp) or falling back to param.cgi
/// on 404 for read operations, or re-raising the error for write operations.
fn post_signedvideo(
    client: &VapixClient,
    body: &Value,
) -> Result<Value, SignedVideoError> {
    match client.post_json("/axis-cgi/signedvideo.cgi", body) {
        Ok(resp) => Ok(resp),
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("404") {
                debug!("signedvideo.cgi returned 404");
                Err(SignedVideoError::NotFound)
            } else {
                Err(SignedVideoError::Other(e))
            }
        }
    }
}

enum SignedVideoError {
    NotFound,
    Other(anyhow::Error),
}

/// Get signed video status.
/// Tries `signedvideo.cgi` first; on 404 falls back to reading signed video
/// parameters via `param.cgi` (root.SignedVideo / root.Properties.SignedVideo).
pub fn get_status(client: &VapixClient) -> anyhow::Result<Value> {
    match post_signedvideo(client, &json!({
        "apiVersion": "1.0",
        "method": "getStatus",
    })) {
        Ok(resp) => Ok(resp),
        Err(SignedVideoError::NotFound) => get_status_from_params(client),
        Err(SignedVideoError::Other(e)) => Err(e),
    }
}

/// The per-channel signed video switches in a `param.cgi` listing of group
/// `Image`: `root.Image.I<n>.MPEG.SignedVideo.Enabled`, which is where the
/// camera keeps the setting (there is no `root.SignedVideo` group).
fn signed_video_switches(listing: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for line in listing.lines() {
        let Some((key, value)) = line.trim().split_once('=') else { continue };
        let Some(rest) = key.strip_prefix("root.Image.I") else { continue };
        let Some((channel, tail)) = rest.split_once('.') else { continue };
        if channel.is_empty() || !channel.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if tail == "MPEG.SignedVideo.Enabled" {
            found.push((key.to_string(), value.trim().to_string()));
        }
    }
    found
}

fn read_switches(client: &VapixClient) -> anyhow::Result<Vec<(String, String)>> {
    let text = client.get_text("/axis-cgi/param.cgi", &[("action", "list"), ("group", "Image")])?;
    if text.trim_start().starts_with("# Error") {
        anyhow::bail!("param.cgi: {}", text.trim());
    }
    Ok(signed_video_switches(&text))
}

/// Fallback: read the signed video switches of every video channel from param.cgi.
fn get_status_from_params(client: &VapixClient) -> anyhow::Result<Value> {
    debug!("Falling back to param.cgi for signed video status");
    let switches = read_switches(client)?;
    if switches.is_empty() {
        anyhow::bail!(
            "Signed video API not available on this camera \
             (signedvideo.cgi returned 404 and no Image.I<n>.MPEG.SignedVideo parameters found). \
             Use 'vapx discover' to check supported APIs."
        );
    }
    let mut params = serde_json::Map::new();
    for (key, value) in &switches {
        let name = key.strip_prefix("root.").unwrap_or(key);
        params.insert(name.to_string(), json!(value));
    }
    Ok(json!({
        "data": {
            "signedVideo": params,
            "source": "param_cgi_fallback",
        }
    }))
}

/// Enable signed video.
/// Tries `signedvideo.cgi` first; on 404 falls back to setting the parameter
/// via `param.cgi`.
pub fn enable(client: &VapixClient) -> anyhow::Result<Value> {
    match post_signedvideo(client, &json!({
        "apiVersion": "1.0",
        "method": "setEnabled",
        "params": {"enabled": true},
    })) {
        Ok(resp) => Ok(resp),
        Err(SignedVideoError::NotFound) => set_enabled_via_params(client, true),
        Err(SignedVideoError::Other(e)) => Err(e),
    }
}

/// Disable signed video.
/// Tries `signedvideo.cgi` first; on 404 falls back to setting the parameter
/// via `param.cgi`.
pub fn disable(client: &VapixClient) -> anyhow::Result<Value> {
    match post_signedvideo(client, &json!({
        "apiVersion": "1.0",
        "method": "setEnabled",
        "params": {"enabled": false},
    })) {
        Ok(resp) => Ok(resp),
        Err(SignedVideoError::NotFound) => set_enabled_via_params(client, false),
        Err(SignedVideoError::Other(e)) => Err(e),
    }
}

/// Fallback: set signed video on or off for every video channel via param.cgi.
fn set_enabled_via_params(client: &VapixClient, enabled: bool) -> anyhow::Result<Value> {
    debug!("Falling back to param.cgi to set signed video enabled={}", enabled);
    let val = if enabled { "yes" } else { "no" };
    let switches = read_switches(client)?;
    if switches.is_empty() {
        anyhow::bail!(
            "Signed video is not available on this camera \
             (signedvideo.cgi returned 404 and no Image.I<n>.MPEG.SignedVideo parameters found)."
        );
    }
    let mut channels = Vec::new();
    for (key, _) in &switches {
        let text = client.get_text("/axis-cgi/param.cgi", &[("action", "update"), (key.as_str(), val)])?;
        if !text.trim().eq_ignore_ascii_case("OK") {
            anyhow::bail!("Failed to set {} via param.cgi: {}", key, text.trim());
        }
        channels.push(key.trim_start_matches("root.").to_string());
    }
    Ok(json!({
        "data": {
            "enabled": enabled,
            "parameters": channels,
            "source": "param_cgi_fallback",
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vapix::testserver::{Reply, TestServer};

    const LISTING: &str = "root.Image.I0.MPEG.SignedVideo.Enabled=no\n\
        root.Image.I1.MPEG.SignedVideo.Enabled=yes\n\
        root.Image.I0.MPEG.PCount=61\n\
        root.Image.I0.Overlay.Enabled=no\n";

    #[test]
    fn finds_the_switch_of_every_channel_and_nothing_else() {
        let found = signed_video_switches(LISTING);
        assert_eq!(
            found,
            vec![
                ("root.Image.I0.MPEG.SignedVideo.Enabled".to_string(), "no".to_string()),
                ("root.Image.I1.MPEG.SignedVideo.Enabled".to_string(), "yes".to_string()),
            ]
        );
        assert!(signed_video_switches("root.Image.Ix.MPEG.SignedVideo.Enabled=no\n").is_empty());
    }

    fn camera_without_signedvideo_cgi() -> TestServer {
        TestServer::start(|req| {
            if req.contains("signedvideo.cgi") {
                Reply::Text(404, "Not Found")
            } else if req.contains("action=update") {
                Reply::Text(200, "OK")
            } else {
                Reply::Text(200, LISTING)
            }
        })
    }

    #[test]
    fn status_falls_back_to_the_image_parameters_on_404() {
        let srv = camera_without_signedvideo_cgi();
        let v = get_status(&srv.client()).unwrap();
        assert_eq!(v["data"]["source"], "param_cgi_fallback");
        assert_eq!(v["data"]["signedVideo"]["Image.I1.MPEG.SignedVideo.Enabled"], "yes");
    }

    #[test]
    fn enable_sets_every_channel_through_param_cgi() {
        let srv = camera_without_signedvideo_cgi();
        let v = enable(&srv.client()).unwrap();
        assert_eq!(v["data"]["enabled"], true);
        let updates: Vec<String> = srv.requests().into_iter().filter(|r| r.contains("action=update")).collect();
        assert_eq!(updates.len(), 2, "{updates:?}");
        assert!(updates[0].contains("root.Image.I0.MPEG.SignedVideo.Enabled=yes"), "{}", updates[0]);
        assert!(updates[1].contains("root.Image.I1.MPEG.SignedVideo.Enabled=yes"), "{}", updates[1]);
    }

    #[test]
    fn a_camera_without_any_switch_says_so() {
        let srv = TestServer::start(|req| {
            if req.contains("signedvideo.cgi") { Reply::Text(404, "Not Found") } else { Reply::Text(200, "root.Image.I0.Overlay.Enabled=no\n") }
        });
        let err = get_status(&srv.client()).unwrap_err().to_string();
        assert!(err.contains("not available"), "{err}");
    }
}
