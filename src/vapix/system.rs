//! System and audit logs.

use crate::vapix::client::VapixClient;

/// The camera's system log, or its last `lines` lines.
pub fn system_log(client: &VapixClient, lines: Option<usize>) -> anyhow::Result<String> {
    Ok(tail(&client.get_text("/axis-cgi/admin/systemlog.cgi", &[])?, lines))
}

/// The security audit log (logins, configuration changes), or its last `lines` lines.
pub fn audit_log(client: &VapixClient, lines: Option<usize>) -> anyhow::Result<String> {
    Ok(tail(&client.get_text("/axis-cgi/auditlog.cgi", &[])?, lines))
}

/// The last `lines` lines of `text`; everything when `lines` is absent or 0.
pub fn tail(text: &str, lines: Option<usize>) -> String {
    match lines {
        Some(n) if n > 0 => {
            let all: Vec<&str> = text.lines().collect();
            all[all.len().saturating_sub(n)..].join("\n")
        }
        _ => text.to_string(),
    }
}

/// Reboot the camera: `firmwaremanagement.cgi`, or the legacy `restart.cgi`
/// when the camera says it does not have that API (404, 405, 501 or "not
/// supported"). Any other failure is reported as it is: falling back after a
/// timeout or an auth error could reboot the camera twice. Each request is
/// sent once; a connection that closes while the camera goes down counts as
/// sent (see `VapixClient::post_json_once`).
pub fn reboot(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    match crate::vapix::firmware::reboot(client) {
        Ok(_) => Ok(serde_json::json!({"status": "rebooting", "method": "firmwaremanagement"})),
        Err(err) if crate::vapix::client::is_not_supported(&err) => {
            client.get_text_once("/axis-cgi/restart.cgi")?;
            Ok(serde_json::json!({"status": "rebooting", "method": "restart.cgi (legacy)"}))
        }
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::{reboot, tail};
    use crate::vapix::testserver::{Reply, TestServer};

    const FW: &str = "POST /axis-cgi/firmwaremanagement.cgi";
    const LEGACY: &str = "GET /axis-cgi/restart.cgi";

    fn count(server: &TestServer, prefix: &str) -> usize {
        server.requests().iter().filter(|r| r.starts_with(prefix)).count()
    }

    #[test]
    fn reboot_uses_the_legacy_endpoint_when_the_new_api_is_missing() {
        let server = TestServer::start(|r| if r.starts_with(FW) { Reply::Text(404, "nope") } else { Reply::Text(200, "ok") });
        let out = reboot(&server.client()).unwrap();
        assert_eq!(out["method"], "restart.cgi (legacy)");
        assert_eq!(count(&server, FW), 1);
        assert_eq!(count(&server, LEGACY), 1);
    }

    #[test]
    fn reboot_does_not_fall_back_after_a_real_failure() {
        // A 500 or a refusal says nothing about missing support; a second
        // reboot attempt could reboot the camera twice.
        for status in [500u16, 403, 503] {
            let server = TestServer::start(move |_| Reply::Text(status, "failed"));
            let err = reboot(&server.client()).unwrap_err();
            assert!(err.to_string().starts_with(&format!("HTTP {status}")), "{err}");
            assert_eq!(count(&server, FW), 1, "status {status}");
            assert_eq!(count(&server, LEGACY), 0, "status {status}");
        }
    }

    #[test]
    fn reboot_counts_a_dropped_connection_as_sent_without_a_second_try() {
        let server = TestServer::start(|_| Reply::Drop);
        let out = reboot(&server.client()).unwrap();
        assert_eq!(out["method"], "firmwaremanagement");
        assert_eq!(count(&server, FW), 1);
        assert_eq!(count(&server, LEGACY), 0);
    }

    #[test]
    fn the_legacy_reboot_is_sent_once_even_when_it_fails() {
        let server = TestServer::start(|r| if r.starts_with(FW) { Reply::Text(404, "nope") } else { Reply::Text(500, "bad") });
        assert!(reboot(&server.client()).is_err());
        assert_eq!(count(&server, LEGACY), 1);
    }

    #[test]
    fn tail_keeps_the_last_lines() {
        assert_eq!(tail("a\nb\nc\nd", Some(2)), "c\nd");
        assert_eq!(tail("a\nb", Some(5)), "a\nb");
    }

    #[test]
    fn tail_without_a_limit_returns_everything() {
        assert_eq!(tail("a\nb\n", None), "a\nb\n");
        assert_eq!(tail("a\nb\n", Some(0)), "a\nb\n");
    }
}
