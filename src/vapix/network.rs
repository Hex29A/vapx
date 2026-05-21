use anyhow::bail;

use crate::vapix::client::VapixClient;

/// Fetch network configuration parameters (root.Network group).
pub fn get_config(client: &VapixClient) -> anyhow::Result<String> {
    let params = [("action", "list"), ("group", "root.Network")];
    let text = client.get_text("/axis-cgi/param.cgi", &params)?;
    if text.starts_with("# Error:") {
        bail!("param.cgi: {}", text.trim());
    }
    Ok(text)
}

/// Update network parameters. Each entry is (param_name, value).
pub fn set_params(client: &VapixClient, assignments: &[(&str, &str)]) -> anyhow::Result<String> {
    let mut params: Vec<(&str, &str)> = vec![("action", "update")];
    params.extend_from_slice(assignments);
    let text = client.get_text("/axis-cgi/param.cgi", &params)?;
    if text.starts_with("# Error:") || text.contains("Error:") {
        bail!("param.cgi: {}", text.trim());
    }
    Ok(text)
}
