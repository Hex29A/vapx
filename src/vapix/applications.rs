use anyhow::bail;

use crate::vapix::client::VapixClient;

/// List installed ACAP applications. Response is XML, parsed into structured data.
pub fn list_applications(client: &VapixClient) -> anyhow::Result<Vec<AcapApp>> {
    let text = client.get_text("/axis-cgi/applications/list.cgi", &[])?;
    parse_application_list(&text)
}

/// Start an application
pub fn control(client: &VapixClient, action: &str, package: &str) -> anyhow::Result<String> {
    let text = client.get_text(
        "/axis-cgi/applications/control.cgi",
        &[("action", action), ("package", package)],
    )?;
    let trimmed = text.trim().to_string();
    if trimmed == "OK" {
        Ok(trimmed)
    } else {
        bail!("ACAP control error: {}", trimmed)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AcapApp {
    pub name: String,
    pub nice_name: String,
    pub vendor: String,
    pub version: String,
    pub status: String,
    pub license: String,
}

fn parse_application_list(xml: &str) -> anyhow::Result<Vec<AcapApp>> {
    let doc = roxmltree::Document::parse(xml)?;
    let mut apps = Vec::new();

    for node in doc.descendants() {
        if node.tag_name().name() == "application" {
            apps.push(AcapApp {
                name: node.attribute("Name").unwrap_or("").to_string(),
                nice_name: node.attribute("NiceName").unwrap_or("").to_string(),
                vendor: node.attribute("Vendor").unwrap_or("").to_string(),
                version: node.attribute("Version").unwrap_or("").to_string(),
                status: node.attribute("Status").unwrap_or("").to_string(),
                license: node.attribute("License").unwrap_or("").to_string(),
            });
        }
    }

    Ok(apps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_application_list() {
        let xml = r#"<reply result="ok">
    <application
        Name="vdo_larod"
        NiceName="Object Analytics"
        Vendor="Axis Communications"
        Version="2.3.4"
        ApplicationID="12345"
        License="None"
        Status="Running"
        ConfigurationPage=""
        SignatureStatus="Signed">
    </application>
    <application
        Name="fenceguard"
        NiceName="Fence Guard"
        Vendor="Axis Communications"
        Version="1.0.0"
        ApplicationID="67890"
        License="Valid"
        Status="Stopped"
        ConfigurationPage=""
        SignatureStatus="Signed">
    </application>
</reply>"#;
        let apps = parse_application_list(xml).unwrap();
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].name, "vdo_larod");
        assert_eq!(apps[0].status, "Running");
        assert_eq!(apps[1].name, "fenceguard");
        assert_eq!(apps[1].status, "Stopped");
    }
}
