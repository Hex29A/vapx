use anyhow::bail;
use serde_json::{json, Value};

use crate::vapix::client::VapixClient;
use crate::vapix::params;

const ENABLED_PARAM: &str = "root.Network.SSH.Enabled";

/// The JSON CGI for the service switch. It has no user methods; the
/// documentation says it yields the same result as the parameter.
const SSH_CGI: &str = "/axis-cgi/ssh.cgi";

/// Device Configuration API for SSH accounts. `ssh.v2` is the released
/// version; an earlier `ssh.v1` (beta, `/config/rest/ssh/v1beta`) also exists.
const USERS_PATH: &str = "/config/rest/ssh/v2/users";

/// Discovery endpoint listing the major versions of the SSH API a device has.
const DISCOVER_PATH: &str = "/config/discover/apis/ssh";

/// Whether the SSH service is switched on. Reads `root.Network.SSH.Enabled`
/// and falls back to `ssh.cgi` when the parameter is not reported.
pub fn is_enabled(client: &VapixClient) -> anyhow::Result<bool> {
    let from_param = params::list(client, Some("root.Network.SSH")).and_then(|t| parse_enabled(&t));
    match from_param {
        Ok(v) => Ok(v),
        Err(param_err) => {
            let resp = call_cgi(client, "getSshInfo", json!({}))
                .map_err(|cgi_err| anyhow::anyhow!("{}; {}", param_err, cgi_err))?;
            resp.pointer("/data/enabled")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| anyhow::anyhow!("{}; ssh.cgi gave no data.enabled", param_err))
        }
    }
}

/// Switch the SSH service on or off. Uses the parameter and falls back to
/// `ssh.cgi` if the device rejects it.
pub fn set_enabled(client: &VapixClient, enabled: bool) -> anyhow::Result<()> {
    let value = if enabled { "yes" } else { "no" };
    if let Err(param_err) = params::update(client, &[(ENABLED_PARAM, value)]) {
        call_cgi(client, "setSshConfiguration", json!({ "enabled": enabled }))
            .map_err(|cgi_err| anyhow::anyhow!("{}; {}", param_err, cgi_err))?;
    }
    Ok(())
}

/// One call to `ssh.cgi`. The CGI reports failures as HTTP 200 with an
/// `error` object, so the body has to be checked.
fn call_cgi(client: &VapixClient, method: &str, params: Value) -> anyhow::Result<Value> {
    let body = json!({ "apiVersion": "1.0", "context": "vapx", "method": method, "params": params });
    let resp = client.rest_json("POST", SSH_CGI, Some(&body))?;
    check_cgi_error(&resp)?;
    Ok(resp)
}

/// Turn an `ssh.cgi` `error` object into an error.
fn check_cgi_error(resp: &Value) -> anyhow::Result<()> {
    if let Some(err) = resp.get("error") {
        let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
        let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("unknown error");
        bail!("ssh.cgi: {} (code {})", msg, code);
    }
    Ok(())
}

/// Major versions of the SSH Device Configuration API the device reports, e.g.
/// `["v1", "v2"]`. `None` when the device has no discovery endpoint.
pub fn api_versions(client: &VapixClient) -> Option<Vec<String>> {
    let resp = client.rest_json("GET", DISCOVER_PATH, None).ok()?;
    Some(parse_versions(&resp))
}

/// Pull the version keys out of a discovery answer. Devices on AXIS OS 12.11
/// and 13.1 answer `{"v2": {...}}` at the top level; the documentation shows
/// `{"ssh": {"v1": {...}, "v2": {...}}}`, and a `data` wrapper is tolerated.
fn parse_versions(resp: &Value) -> Vec<String> {
    let node = resp
        .pointer("/data/ssh")
        .or_else(|| resp.get("ssh"))
        .or_else(|| resp.get("data"))
        .unwrap_or(resp);
    let mut versions: Vec<String> = node
        .as_object()
        .map(|o| o.keys().filter(|k| is_version_key(k)).cloned().collect())
        .unwrap_or_default();
    versions.sort();
    versions
}

/// `v1`, `v2`, `v1beta` — but not `version` or other fields.
fn is_version_key(k: &str) -> bool {
    k.strip_prefix('v')
        .and_then(|r| r.chars().next())
        .is_some_and(|c| c.is_ascii_digit())
}

/// What a 404 from the account API means depends on whether discovery found
/// `v2`. Without it the device has no account API at all (AXIS OS 9 and 10
/// have no Device Configuration API), so say that instead of a bare
/// "HTTP 404". With it, a 404 on `users/<name>` is a missing account.
fn explain_404(e: anyhow::Error, has_v2: bool, name: Option<&str>) -> anyhow::Error {
    if !e.to_string().contains("HTTP 404") {
        return e;
    }
    match (has_v2, name) {
        (true, Some(n)) => anyhow::anyhow!("No SSH account named '{}' on this device", n),
        (true, None) => e,
        (false, _) => anyhow::anyhow!(
            "This device has no SSH account API v2 (HTTP 404). Upgrade AXIS OS or create the account in the web interface."
        ),
    }
}

/// The account API is only in `ssh.v2`. Stop with a clear message if the device
/// says it has other versions but not that one; if discovery itself is not
/// available, try the call anyway and let the device answer. Returns whether
/// the device reported `v2`.
fn require_users_api(client: &VapixClient) -> anyhow::Result<bool> {
    match api_versions(client) {
        Some(v) if !v.is_empty() && !v.iter().any(|x| x == "v2") => bail!(
            "This device has no SSH account API v2 (found: {}). Upgrade AXIS OS or create the account in the web interface.",
            v.join(", ")
        ),
        Some(v) => Ok(!v.is_empty()),
        None => Ok(false),
    }
}

/// List SSH accounts as the `data` array of the REST answer.
pub fn list_users(client: &VapixClient) -> anyhow::Result<Vec<Value>> {
    let has_v2 = require_users_api(client)?;
    let resp = client
        .rest_json("GET", USERS_PATH, None)
        .map_err(|e| explain_404(e, has_v2, None))?;
    Ok(resp
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default())
}

/// Create an SSH account. The password travels in the JSON body, never in the URL.
pub fn add_user(
    client: &VapixClient,
    name: &str,
    pwd: &str,
    comment: &str,
) -> anyhow::Result<()> {
    check_name(name)?;
    let has_v2 = require_users_api(client)?;
    let body = json!({"data": {"username": name, "password": pwd, "comment": comment}});
    client
        .rest_json("POST", USERS_PATH, Some(&body))
        .map_err(|e| explain_404(e, has_v2, None))?;
    Ok(())
}

/// Change the password of an existing SSH account.
pub fn set_password(client: &VapixClient, name: &str, pwd: &str) -> anyhow::Result<()> {
    check_name(name)?;
    let has_v2 = require_users_api(client)?;
    let body = json!({"data": pwd});
    client
        .rest_json("PATCH", &format!("{}/{}/password", USERS_PATH, name), Some(&body))
        .map_err(|e| explain_404(e, has_v2, Some(name)))?;
    Ok(())
}

/// Delete an SSH account.
pub fn remove_user(client: &VapixClient, name: &str) -> anyhow::Result<()> {
    check_name(name)?;
    let has_v2 = require_users_api(client)?;
    client
        .rest_json("DELETE", &format!("{}/{}", USERS_PATH, name), None)
        .map_err(|e| explain_404(e, has_v2, Some(name)))?;
    Ok(())
}

/// The account name becomes a URL path segment, so only characters that need
/// no escaping are allowed.
fn check_name(name: &str) -> anyhow::Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if !ok {
        bail!("Invalid SSH account name '{}': use 1-32 of a-z A-Z 0-9 _ - .", name);
    }
    Ok(())
}

/// Pick the SSH switch out of a param.cgi listing.
fn parse_enabled(text: &str) -> anyhow::Result<bool> {
    for line in text.lines() {
        if let Some((k, v)) = line.trim().split_once('=') {
            if k == ENABLED_PARAM {
                return Ok(v.trim() == "yes");
            }
        }
    }
    bail!("{} not reported by the device", ENABLED_PARAM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_enabled_yes() {
        let text = "root.Network.SSH.Enabled=yes\nroot.Network.SSH.Port=22\n";
        assert!(parse_enabled(text).unwrap());
    }

    #[test]
    fn test_parse_enabled_no() {
        assert!(!parse_enabled("root.Network.SSH.Enabled=no\n").unwrap());
    }

    #[test]
    fn test_parse_enabled_missing_is_error() {
        assert!(parse_enabled("root.Network.SSH.Port=22\n").is_err());
    }

    #[test]
    fn test_parse_versions_plain_and_wrapped() {
        let plain = json!({"ssh": {"v2": {}, "v1": {}}});
        assert_eq!(parse_versions(&plain), vec!["v1", "v2"]);
        let wrapped = json!({"status": "success", "data": {"ssh": {"v2": {}}}});
        assert_eq!(parse_versions(&wrapped), vec!["v2"]);
        assert!(parse_versions(&json!({})).is_empty());
    }

    #[test]
    fn test_parse_versions_top_level_as_devices_answer() {
        // Verbatim shape from a Q3556-LVE on AXIS OS 13.1.34.
        let dev = json!({"v2": {"rest_api": "/config/rest/ssh/v2", "state": "released", "version": "2.0.5"}});
        assert_eq!(parse_versions(&dev), vec!["v2"]);
        let beta = json!({"v1beta": {}, "version": "x", "v2": {}});
        assert_eq!(parse_versions(&beta), vec!["v1beta", "v2"]);
    }

    #[test]
    fn test_404_is_explained_by_discovery() {
        let nf = || anyhow::anyhow!("HTTP 404: 404 Not Found");
        // No v2 reported (AXIS OS 9.80): the API is missing.
        assert!(explain_404(nf(), false, Some("x")).to_string().contains("no SSH account API v2"));
        // v2 reported: a 404 on users/<name> is a missing account.
        assert!(explain_404(nf(), true, Some("x")).to_string().contains("No SSH account named 'x'"));
        assert!(explain_404(nf(), true, None).to_string().contains("HTTP 404"));
        let other = explain_404(anyhow::anyhow!("HTTP 401: Unauthorized"), false, None);
        assert!(other.to_string().contains("401"));
    }

    #[test]
    fn test_cgi_error_is_reported() {
        let bad = json!({"apiVersion": "1.0", "error": {"code": 4003, "message": "Missing parameter(s)"}});
        let e = check_cgi_error(&bad).unwrap_err().to_string();
        assert!(e.contains("Missing parameter") && e.contains("4003"));
        assert!(check_cgi_error(&json!({"data": {}})).is_ok());
    }

    #[test]
    fn test_check_name_accepts_plain_names() {
        assert!(check_name("support").is_ok());
        assert!(check_name("a_b-c.d9").is_ok());
    }

    #[test]
    fn test_check_name_rejects_path_tricks() {
        assert!(check_name("").is_err());
        assert!(check_name("../x").is_err());
        assert!(check_name("a/b").is_err());
        assert!(check_name("a b").is_err());
        assert!(check_name(&"x".repeat(33)).is_err());
    }
}
