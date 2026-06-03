use anyhow::bail;

use crate::vapix::client::VapixClient;

/// List available ZipStream profiles via the XML CGI.
/// Returns the raw XML response.
pub fn list_profiles(client: &VapixClient) -> anyhow::Result<String> {
    let text = client.get_text("/axis-cgi/zipstream/listprofiles.cgi", &[])?;
    let lower = text.to_lowercase();
    if lower.contains("<error>") || lower.contains("not found") {
        bail!("ZipStream API not available on this camera. Use 'vapx discover' to check supported APIs.");
    }
    Ok(text)
}

/// Set ZipStream profile and level.
/// profile: "classic", "storage", or "networkloadbalancing"
/// level: 0-100 (strength)
pub fn set_profile(client: &VapixClient, profile: &str, level: u32) -> anyhow::Result<String> {
    let text = client.get_text(
        "/axis-cgi/zipstream/setprofile.cgi",
        &[("profile", profile), ("level", &level.to_string())],
    )?;
    let lower = text.to_lowercase();
    if lower.contains("<error>") || lower.contains("not found") {
        bail!("ZipStream API not available on this camera. Use 'vapx discover' to check supported APIs.");
    }
    Ok(text)
}
