use anyhow::bail;

use crate::vapix::client::VapixClient;

/// List users and their group memberships (action=get).
pub fn list(client: &VapixClient) -> anyhow::Result<String> {
    let text = client.get_text("/axis-cgi/pwdgrp.cgi", &[("action", "get")])?;
    if text.contains("Error:") {
        bail!("pwdgrp.cgi: {}", text.trim());
    }
    Ok(text)
}

/// Add a new user account.
pub fn add(
    client: &VapixClient,
    user: &str,
    pwd: &str,
    sgrp: &str,
    comment: &str,
) -> anyhow::Result<String> {
    let params = [
        ("action", "add"),
        ("user", user),
        ("pwd", pwd),
        ("grp", "users"),
        ("sgrp", sgrp),
        ("comment", comment),
    ];
    let text = client.get_text("/axis-cgi/pwdgrp.cgi", &params)?;
    if text.contains("Error:") {
        bail!("pwdgrp.cgi: {}", text.trim());
    }
    Ok(text)
}

/// Update an existing user account (change password).
pub fn update(client: &VapixClient, user: &str, pwd: &str) -> anyhow::Result<String> {
    let params = [("action", "update"), ("user", user), ("pwd", pwd)];
    let text = client.get_text("/axis-cgi/pwdgrp.cgi", &params)?;
    if text.contains("Error:") {
        bail!("pwdgrp.cgi: {}", text.trim());
    }
    Ok(text)
}

/// Remove a user account.
pub fn remove(client: &VapixClient, user: &str) -> anyhow::Result<String> {
    let params = [("action", "remove"), ("user", user)];
    let text = client.get_text("/axis-cgi/pwdgrp.cgi", &params)?;
    if text.contains("Error:") {
        bail!("pwdgrp.cgi: {}", text.trim());
    }
    Ok(text)
}
