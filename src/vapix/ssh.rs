use anyhow::bail;
use serde_json::{json, Value};

use crate::vapix::client::VapixClient;
use crate::vapix::params;

const ENABLED_PARAM: &str = "root.Network.SSH.Enabled";

/// Device Configuration API for SSH accounts (AXIS OS 12). The older
/// `ssh.cgi` has no user methods, so accounts can only be managed here.
const USERS_PATH: &str = "/config/rest/ssh/v2/users";

/// Whether the SSH service is switched on (`root.Network.SSH.Enabled`).
pub fn is_enabled(client: &VapixClient) -> anyhow::Result<bool> {
    let text = params::list(client, Some("root.Network.SSH"))?;
    parse_enabled(&text)
}

/// Switch the SSH service on or off.
pub fn set_enabled(client: &VapixClient, enabled: bool) -> anyhow::Result<()> {
    let value = if enabled { "yes" } else { "no" };
    params::update(client, &[(ENABLED_PARAM, value)])?;
    Ok(())
}

/// List SSH accounts as the `data` array of the REST answer.
pub fn list_users(client: &VapixClient) -> anyhow::Result<Vec<Value>> {
    let resp = client.rest_json("GET", USERS_PATH, None)?;
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
    let body = json!({"data": {"username": name, "password": pwd, "comment": comment}});
    client.rest_json("POST", USERS_PATH, Some(&body))?;
    Ok(())
}

/// Change the password of an existing SSH account.
pub fn set_password(client: &VapixClient, name: &str, pwd: &str) -> anyhow::Result<()> {
    check_name(name)?;
    let body = json!({"data": pwd});
    client.rest_json("PATCH", &format!("{}/{}/password", USERS_PATH, name), Some(&body))?;
    Ok(())
}

/// Delete an SSH account.
pub fn remove_user(client: &VapixClient, name: &str) -> anyhow::Result<()> {
    check_name(name)?;
    client.rest_json("DELETE", &format!("{}/{}", USERS_PATH, name), None)?;
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
