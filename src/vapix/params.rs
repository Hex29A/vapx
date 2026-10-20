use anyhow::bail;

use crate::vapix::client::VapixClient;

/// List parameters, optionally filtered by group.
pub fn list(client: &VapixClient, group: Option<&str>) -> anyhow::Result<String> {
    let mut params = vec![("action", "list")];
    if let Some(g) = group {
        params.push(("group", g));
    }
    let text = client.get_text("/axis-cgi/param.cgi", &params)?;
    if text.starts_with("# Error:") {
        bail!("param.cgi: {}", text.trim());
    }
    Ok(text)
}

/// Update one or more parameters. Each entry is (param_name, value).
pub fn update(client: &VapixClient, assignments: &[(&str, &str)]) -> anyhow::Result<String> {
    let mut params: Vec<(&str, &str)> = vec![("action", "update")];
    params.extend_from_slice(assignments);
    let text = client.get_text("/axis-cgi/param.cgi", &params)?;
    if text.starts_with("# Error:") || text.contains("Error:") {
        bail!("param.cgi: {}", text.trim());
    }
    Ok(text)
}

/// `key=value` lines as a sorted map. Blank lines and `#` comments are skipped.
pub fn parse(text: &str) -> std::collections::BTreeMap<String, String> {
    let mut map = std::collections::BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.to_string(), v.to_string());
        }
    }
    map
}

/// The differences between two parameter sets, sorted by parameter name.
/// Each entry has `param` and `change` (`modified`, `only_in_first` or
/// `only_in_second`) and the value(s) under the names `name_a` / `name_b`.
pub fn diff(
    name_a: &str,
    name_b: &str,
    a: &std::collections::BTreeMap<String, String>,
    b: &std::collections::BTreeMap<String, String>,
) -> Vec<serde_json::Value> {
    use serde_json::json;
    let mut diffs = Vec::new();
    for (k, v_a) in a {
        match b.get(k) {
            Some(v_b) if v_a != v_b => diffs.push(json!({
                "param": k, "change": "modified", name_a: v_a, name_b: v_b,
            })),
            None => diffs.push(json!({"param": k, "change": "only_in_first", name_a: v_a})),
            _ => {}
        }
    }
    for (k, v_b) in b {
        if !a.contains_key(k) {
            diffs.push(json!({"param": k, "change": "only_in_second", name_b: v_b}));
        }
    }
    diffs.sort_by(|x, y| x["param"].as_str().unwrap_or("").cmp(y["param"].as_str().unwrap_or("")));
    diffs
}

/// Whether a parameter name looks like it holds a secret (password, token,
/// community string, key material ...).
pub fn is_sensitive(key: &str) -> bool {
    let k = key.to_lowercase();
    ["password", "passwd", "pwd", "passphrase", "secret", "token", "community", "apikey", "privatekey", "psk", "credential"]
        .iter()
        .any(|word| k.contains(word))
}

/// The value to show for `key`: the value itself, or a marker for secrets.
pub fn shown(key: &str, value: &str) -> String {
    if is_sensitive(key) { "[redacted]".to_string() } else { value.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skips_comments_and_keeps_equals_in_values() {
        let m = parse("# c

root.A=1
root.U=http://x/?a=1&b=2
");
        assert_eq!(m.len(), 2);
        assert_eq!(m["root.U"], "http://x/?a=1&b=2");
    }

    #[test]
    fn diff_reports_changed_and_one_sided_keys_in_order() {
        let a = parse("root.A=1
root.B=2
root.C=3");
        let b = parse("root.A=1
root.B=9
root.D=4");
        let d = diff("a", "b", &a, &b);
        let got: Vec<(&str, &str)> = d.iter().map(|e| (e["param"].as_str().unwrap(), e["change"].as_str().unwrap())).collect();
        assert_eq!(got, [("root.B", "modified"), ("root.C", "only_in_first"), ("root.D", "only_in_second")]);
        assert_eq!(d[0]["a"], "2");
        assert_eq!(d[0]["b"], "9");
    }

    #[test]
    fn secrets_are_recognised_by_name() {
        for k in ["root.MQTT.Password", "root.SNMP.V2.ReadCommunity", "root.Foo.ApiKey", "root.X.Token"] {
            assert!(is_sensitive(k), "{k}");
            assert_eq!(shown(k, "hunter2"), "[redacted]");
        }
        assert!(!is_sensitive("root.Brand.ProdNbr"));
        assert_eq!(shown("root.Brand.ProdNbr", "P1375"), "P1375");
    }
}
