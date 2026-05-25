use crate::vapix::client::VapixClient;
use serde_json::json;
use tracing::debug;

/// Attempt a certificate API call, providing a clear error on 404.
fn cert_request(client: &VapixClient, body: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    match client.post_json("/axis-cgi/certificate/certificate.cgi", body) {
        Ok(resp) => Ok(resp),
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("404") {
                debug!("Certificate API returned 404, trying legacy path");
                // Try alternative path used by some firmware versions
                match client.post_json("/axis-cgi/certmanagement.cgi", body) {
                    Ok(resp) => Ok(resp),
                    Err(_) => anyhow::bail!(
                        "Certificate management API not available on this camera. \
                         This API may require a different firmware version or is only \
                         accessible via SOAP (/vapix/services). Use 'vapx discover' to \
                         check supported APIs."
                    ),
                }
            } else {
                Err(e)
            }
        }
    }
}

/// List installed certificates.
pub fn list(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    cert_request(
        client,
        &json!({
            "apiVersion": "1.0",
            "method": "getCertificateList",
        }),
    )
}

/// Get certificate info by ID.
pub fn info(client: &VapixClient, cert_id: &str) -> anyhow::Result<serde_json::Value> {
    cert_request(
        client,
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
    cert_request(
        client,
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
    cert_request(
        client,
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
    cert_request(
        client,
        &json!({
            "apiVersion": "1.0",
            "method": "generateCertificateSigningRequest",
            "params": params,
        }),
    )
}
