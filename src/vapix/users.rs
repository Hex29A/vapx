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

    let text = strip_markup(&client.post_form_text("/axis-cgi/pwdgrp.cgi", &params)?);
    check_error(&text)?;
    Ok(text)
}

/// Update an existing user account (change password).
pub fn update(client: &VapixClient, user: &str, pwd: &str) -> anyhow::Result<String> {
    let params = [("action", "update"), ("user", user), ("pwd", pwd)];
    let text = strip_markup(&client.post_form_text("/axis-cgi/pwdgrp.cgi", &params)?);
    check_error(&text)?;
    Ok(text)
}

/// Remove a user account.
pub fn remove(client: &VapixClient, user: &str) -> anyhow::Result<String> {
    let params = [("action", "remove"), ("user", user)];
    let text = strip_markup(&client.get_text("/axis-cgi/pwdgrp.cgi", &params)?);
    check_error(&text)?;
    Ok(text)
}

/// What removing an account would do, worked out from the account list before anything is sent.
#[derive(Debug, PartialEq, Eq)]
pub struct Removal {
    /// The account is in the list at all.
    pub exists: bool,
    /// The groups the account belongs to.
    pub groups: Vec<String>,
    /// It is the account vapx logs in with, so the next call would get 401.
    pub is_login: bool,
    /// It is the only account of the admin group that a person can log in with.
    pub last_admin: bool,
    /// The accounts the camera has, for the error when the name is not one of them.
    pub accounts: Vec<String>,
}

impl Removal {
    /// Why this removal would cut the user off from the camera, if it would.
    pub fn lockout(&self) -> Option<String> {
        if self.is_login {
            Some("it is the account vapx is logged in with, so the next call to the camera would fail".to_string())
        } else if self.last_admin {
            Some("it is the last admin account, and nobody could manage the camera afterwards".to_string())
        } else {
            None
        }
    }

    pub fn to_json(&self, name: &str) -> serde_json::Value {
        serde_json::json!({
            "account": name,
            "groups": self.groups,
            "isLoginAccount": self.is_login,
            "isLastAdmin": self.last_admin,
            "wouldLockOut": self.lockout().is_some(),
        })
    }
}

/// Look `name` up in the text of [`list`]. `login` is the account vapx logs in with.
pub fn plan_removal(list_text: &str, name: &str, login: &str) -> Removal {
    let parsed = parse_users(list_text);
    let accounts: Vec<(String, Vec<String>, bool)> = parsed["users"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|u| {
                    let groups = u["groups"].as_array().map(|g| g.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
                    (u["username"].as_str().unwrap_or("").to_string(), groups, u["is_service_account"].as_bool().unwrap_or(false))
                })
                .collect()
        })
        .unwrap_or_default();
    let me = accounts.iter().find(|(n, _, _)| n == name);
    let groups = me.map(|(_, g, _)| g.clone()).unwrap_or_default();
    let other_admins = accounts.iter().filter(|(n, g, service)| n != name && !service && g.iter().any(|x| x == "admin")).count();
    Removal {
        exists: me.is_some(),
        last_admin: groups.iter().any(|g| g == "admin") && other_admins == 0,
        groups,
        is_login: name == login,
        accounts: accounts.into_iter().filter(|(_, _, service)| !service).map(|(n, _, _)| n).collect(),
    }
}

/// AXIS OS 10.x answers pwdgrp.cgi writes with a small HTML page
/// (`<html>...Created account x.\r\n</body></html>`). Reduce it to its text.
/// Text without markup is returned as it is.
fn strip_markup(text: &str) -> String {
    if !text.contains('<') {
        return text.to_string();
    }
    let mut out = String::new();
    let mut in_tag = false;
    let mut in_title = false;
    let mut tag = String::new();
    for c in text.chars() {
        match c {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let t = tag.trim().to_ascii_lowercase();
                if t == "title" {
                    in_title = true;
                } else if t == "/title" {
                    in_title = false;
                }
            }
            _ if in_tag => tag.push(c),
            _ if in_title => {}
            _ => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Names like `wwwa`, `wwwaop`, `wwwaovp`: the web-access groups AXIS OS 10.x
/// lists next to the real accounts in the role lists (`www` plus one letter
/// each for admin/operator/viewer/PTZ). They are groups, not people.
pub fn is_web_access_group(name: &str) -> bool {
    name.strip_prefix("www").is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| matches!(c, 'a' | 'o' | 'v' | 'p')))
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

/// The groups that hold people's accounts on every AXIS OS release.
const ACCOUNT_GROUPS: &[&str] = &["root", "users", "admin", "operator", "viewer", "ptz", "digusers"];

/// The names in the `list` text that are accounts, when the text needs telling
/// apart. AXIS OS 10.x prints every system group (`vdo`, `sdk`, `ptzadm`, ...)
/// and puts system users such as `sdk` and `debugar` in the role lines; there
/// the accounts are `root` and the members of `users`. Newer firmware prints
/// only the role groups, and every member is an account: `None`.
pub fn accounts_in(text: &str) -> Option<std::collections::HashSet<String>> {
    let mut users = None;
    let mut system_groups = false;
    for line in text.lines() {
        let Some((group, rest)) = line.trim().split_once('=') else { continue };
        let group = group.trim();
        if group == "users" {
            users = Some(rest.trim_matches('"').split(',').map(|u| u.trim().to_string()).filter(|u| !u.is_empty()).collect::<std::collections::HashSet<_>>());
        } else if !ACCOUNT_GROUPS.contains(&group) {
            system_groups = true;
        }
    }
    if !system_groups {
        return None;
    }
    let mut accounts = users?;
    accounts.insert("root".to_string());
    Some(accounts)
}

/// `{groups: {group: [user]}, users: [{username, groups, is_service_account}]}`
/// from the `list` text (`users="root,viewer"` lines), users sorted by name.
/// Web-access groups and, on firmware that lists them, system users and
/// system groups are left out (see [`accounts_in`]).
pub fn parse_users(text: &str) -> serde_json::Value {
    let accounts = accounts_in(text);
    let mut groups = serde_json::Map::new();
    let mut by_user: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for line in text.lines() {
        let Some((group, rest)) = line.trim().split_once('=') else { continue };
        let group = group.trim();
        if accounts.is_some() && !ACCOUNT_GROUPS.contains(&group) {
            continue;
        }
        let members: Vec<String> = rest
            .trim_matches('"')
            .split(',')
            .map(|u| u.trim().to_string())
            .filter(|u| !u.is_empty() && !is_web_access_group(u))
            .filter(|u| accounts.as_ref().is_none_or(|a| a.contains(u)))
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

#[cfg(test)]
mod removal_tests {
    use super::*;

    const LIST: &str = "root=\"root,admin1\"\nadmin=\"root,admin1,stsuser\"\noperator=\"root,admin1,ops\"\nviewer=\"root,admin1,ops,view\"\nusers=\"root,admin1,ops,view,stsuser\"\n";

    #[test]
    fn an_ordinary_account_can_be_removed() {
        let r = plan_removal(LIST, "view", "root");
        assert!(r.exists && r.lockout().is_none());
        assert_eq!(r.groups, vec!["viewer", "users"]);
    }

    #[test]
    fn the_login_account_is_a_lockout() {
        let r = plan_removal(LIST, "root", "root");
        assert!(r.is_login);
        assert!(r.lockout().unwrap().contains("logged in"));
    }

    #[test]
    fn the_last_admin_is_a_lockout_but_not_while_another_admin_is_left() {
        // admin1 is an admin and so is root: removing admin1 leaves root.
        assert!(!plan_removal(LIST, "admin1", "root").last_admin);
        // With only one human admin left, removing it locks everyone out; the service account does not count.
        let one = "admin=\"solo,stsuser\"\nusers=\"solo,stsuser\"\n";
        let r = plan_removal(one, "solo", "someone-else");
        assert!(r.last_admin);
        assert!(r.lockout().unwrap().contains("last admin"));
    }

    #[test]
    fn an_account_that_is_not_there_does_not_exist() {
        let r = plan_removal(LIST, "ghost", "root");
        assert!(!r.exists);
        assert!(r.accounts.contains(&"admin1".to_string()) && !r.accounts.contains(&"stsuser".to_string()));
    }
}

#[cfg(test)]
mod firmware_10_tests {
    use super::*;

    /// Role lines as AXIS OS 10.12 prints them: group names mixed with accounts.
    const LIST_10_12: &str = "users=\"alice\"\nadmin=\"wwwaop,wwwaov,alice,wwwap,root,wwwa,wwwao,wwwav,wwwaovp,wwwavp\"\noperator=\"vdo,wwwovp,wwwo,root,sdk,alice\"\nviewer=\"wwwv,wwwov,sdk,alice\"\n";

    #[test]
    fn web_access_groups_are_told_from_accounts() {
        for g in ["wwwa", "wwwaop", "wwwaovp", "wwwv", "wwwovp"] {
            assert!(is_web_access_group(g), "{g}");
        }
        for a in ["www", "www-data", "wwwadmin", "alice", "root", "wwwx", ""] {
            assert!(!is_web_access_group(a), "{a}");
        }
    }

    #[test]
    fn parsed_users_do_not_include_web_access_groups() {
        let v = parse_users(LIST_10_12);
        let names: Vec<&str> = v["users"].as_array().unwrap().iter().map(|u| u["username"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["alice", "root", "sdk", "vdo"]);
        assert_eq!(v["groups"]["admin"], serde_json::json!(["alice", "root"]));
    }

    #[test]
    fn removal_planning_ignores_web_access_groups_as_admins() {
        // alice is the only real admin besides root; wwwa* must not count as other admins.
        let r = plan_removal("admin=\"wwwa,wwwaop,alice\"\nusers=\"alice\"\n", "alice", "root");
        assert!(r.last_admin);
    }

    #[test]
    fn an_html_answer_is_reduced_to_its_message() {
        let html = "<html><head><title>User accounts</title></head><body bgcolor=#ffffff>Created account vapxtest.\r\n</body>\r\n\r\n</html>\r\n";
        assert_eq!(strip_markup(html), "Created account vapxtest.");
        assert_eq!(strip_markup("Created account x.\n"), "Created account x.\n");
    }

    #[test]
    fn an_html_error_page_is_still_an_error() {
        let html = "<html><body>Error: Account exists\r\n</body></html>";
        assert!(check_error(&strip_markup(html)).is_err());
    }
}
