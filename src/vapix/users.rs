use anyhow::bail;

use crate::vapix::client::VapixClient;

/// Primary group for a new account.
///
/// Axis recommends `users` for ordinary accounts and documents `root` as
/// intended only for the very first account on a factory-default device.
#[derive(Clone, Copy, PartialEq)]
pub enum PrimaryGroup {
    Users,
    Root,
}

impl PrimaryGroup {
    fn as_str(&self) -> &'static str {
        match self {
            PrimaryGroup::Users => "users",
            PrimaryGroup::Root => "root",
        }
    }
}

/// List users and their group memberships (action=get).
pub fn list(client: &VapixClient) -> anyhow::Result<String> {
    let text = client.get_text("/axis-cgi/pwdgrp.cgi", &[("action", "get")])?;
    check_error(&text)?;
    Ok(text)
}

/// Add a new user account.
///
/// Sent as a POST form: the password would otherwise sit in the query string,
/// which lands in the camera's access log. Axis documents this explicitly —
/// "It is not advisable to create user access data in the URL".
pub fn add(
    client: &VapixClient,
    user: &str,
    pwd: &str,
    sgrp: &str,
    comment: &str,
    grp: PrimaryGroup,
    strict_pwd: bool,
) -> anyhow::Result<String> {
    let mut params: Vec<(&str, &str)> = vec![
        ("action", "add"),
        ("user", user),
        ("pwd", pwd),
        ("grp", grp.as_str()),
        ("sgrp", sgrp),
    ];
    // On AXIS OS older than 11.5 the initial account rejects a comment
    // outright, so only send the field when it carries something.
    if !comment.is_empty() {
        params.push(("comment", comment));
    }
    if strict_pwd {
        params.push(("strict_pwd", "1"));
    }

    let text = client.post_form_text("/axis-cgi/pwdgrp.cgi", &params)?;
    check_error(&text)?;
    Ok(text)
}

/// Update an existing user account (change password).
pub fn update(client: &VapixClient, user: &str, pwd: &str) -> anyhow::Result<String> {
    let params = [("action", "update"), ("user", user), ("pwd", pwd)];
    let text = client.post_form_text("/axis-cgi/pwdgrp.cgi", &params)?;
    check_error(&text)?;
    Ok(text)
}

/// Remove a user account.
pub fn remove(client: &VapixClient, user: &str) -> anyhow::Result<String> {
    let params = [("action", "remove"), ("user", user)];
    let text = client.get_text("/axis-cgi/pwdgrp.cgi", &params)?;
    check_error(&text)?;
    Ok(text)
}

/// pwdgrp.cgi answers 200 with an "Error:" body on failure.
fn check_error(text: &str) -> anyhow::Result<()> {
    if text.contains("Error:") {
        bail!("pwdgrp.cgi: {}", text.trim());
    }
    Ok(())
}

/// Accounts created by Axis services rather than people.
const SERVICE_ACCOUNTS: &[&str] = &["stsuser", "AxisDeviceMgmt", "pfagentvapix"];

/// `{groups: {group: [user]}, users: [{username, groups, is_service_account}]}`
/// from the `list` text (`users="root,viewer"` lines), users sorted by name.
pub fn parse_users(text: &str) -> serde_json::Value {
    let mut groups = serde_json::Map::new();
    let mut by_user: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for line in text.lines() {
        let Some((group, rest)) = line.trim().split_once('=') else { continue };
        let group = group.trim();
        let members: Vec<String> = rest
            .trim_matches('"')
            .split(',')
            .map(|u| u.trim().to_string())
            .filter(|u| !u.is_empty())
            .collect();
        for m in &members {
            by_user.entry(m.clone()).or_default().push(group.to_string());
        }
        groups.insert(group.to_string(), serde_json::json!(members));
    }
    let users: Vec<serde_json::Value> = by_user
        .into_iter()
        .map(|(name, groups)| {
            serde_json::json!({
                "username": name,
                "groups": groups,
                "is_service_account": SERVICE_ACCOUNTS.contains(&name.as_str()),
            })
        })
        .collect();
    serde_json::json!({"groups": groups, "users": users})
}

/// Accounts with their groups (see [`parse_users`]).
pub fn get_users(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    Ok(parse_users(&list(client)?))
}
