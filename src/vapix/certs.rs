use crate::vapix::client::VapixClient;
use serde_json::json;

/// List installed certificates.
pub fn list(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    client.post_json(
        "/axis-cgi/certificate/certificate.cgi",
        &json!({
            "apiVersion": "1.0",
            "method": "getCertificateList",
        }),
    )
}

/// Get certificate info by ID.
pub fn info(client: &VapixClient, cert_id: &str) -> anyhow::Result<serde_json::Value> {
    client.post_json(
        "/axis-cgi/certificate/certificate.cgi",
        &json!({
            "apiVersion": "1.0",
            "method": "getCertificateInfo",
            "params": {
                "id": cert_id,
            },
        }),
    )
}

/// Create a self-signed certificate.
pub fn create_self_signed(
    client: &VapixClient,
    common_name: &str,
    days: u32,
) -> anyhow::Result<serde_json::Value> {
    client.post_json(
        "/axis-cgi/certificate/certificate.cgi",
        &json!({
            "apiVersion": "1.0",
            "method": "generateSelfSignedCertificate",
            "params": {
                "commonName": common_name,
                "validDays": days,
            },
        }),
    )
}

/// Remove a certificate by ID.
pub fn remove(client: &VapixClient, cert_id: &str) -> anyhow::Result<serde_json::Value> {
    client.post_json(
        "/axis-cgi/certificate/certificate.cgi",
        &json!({
            "apiVersion": "1.0",
            "method": "removeCertificate",
            "params": {
                "id": cert_id,
            },
        }),
    )
}

/// Create a Certificate Signing Request (CSR).
pub fn create_csr(
    client: &VapixClient,
    common_name: &str,
    country: Option<&str>,
    organization: Option<&str>,
) -> anyhow::Result<serde_json::Value> {
    let mut params = serde_json::Map::new();
    params.insert("commonName".to_string(), json!(common_name));
    if let Some(c) = country {
        params.insert("country".to_string(), json!(c));
    }
    if let Some(o) = organization {
        params.insert("organization".to_string(), json!(o));
    }
    client.post_json(
        "/axis-cgi/certificate/certificate.cgi",
        &json!({
            "apiVersion": "1.0",
            "method": "generateCertificateSigningRequest",
            "params": params,
        }),
    )
}
