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
