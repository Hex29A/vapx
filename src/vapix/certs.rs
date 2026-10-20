use crate::vapix::client::VapixClient;
use serde_json::json;
use tracing::debug;

/// Attempt a call to the legacy certificate API (before AXIS OS 12), providing a clear error on 404.
fn legacy_request(client: &VapixClient, body: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
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
                        "Certificate management API not available on this camera: neither \
                         /config/rest/cert/v1 nor the legacy certificate.cgi answered. \
                         Use 'vapx discover' to check supported APIs."
                    ),
                }
            } else {
                Err(e)
            }
        }
    }
}

/// Base of the certificate REST API (AXIS OS 12 and later; Device Configuration framework).
const REST_BASE: &str = "/config/rest/cert/v1";

/// List installed certificates.
///
/// Uses the REST API `/config/rest/cert/v1`; cameras without it (older AXIS OS)
/// are asked through the legacy `certificate.cgi`. The REST answer is
/// summarised: `{certificates, ca_certificates, keystores, default_keystore}`,
/// each certificate with `alias`, `subject`, `issuer`, `not_before`,
/// `not_after`, `expired` and `serial` (the PEM text comes with `cert info`).
pub fn list(client: &VapixClient) -> anyhow::Result<serde_json::Value> {
    match client.rest_json("GET", REST_BASE, None) {
        Ok(resp) => {
            let data = resp.get("data").cloned().unwrap_or_default();
            if data.get("certificates").is_some() || data.get("ca_certificates").is_some() {
                return Ok(json!({"data": summarize_list(&data)}));
            }
            debug!("{} answered without certificates, trying the legacy API", REST_BASE);
        }
        Err(e) => debug!("Certificate REST API failed ({}), trying the legacy API", e),
    }
    legacy_request(
        client,
        &json!({
            "apiVersion": "1.0",
            "method": "getCertificateList",
        }),
    )
}

/// Get one certificate by alias (REST) or ID (legacy), with its PEM text.
pub fn info(client: &VapixClient, cert_id: &str) -> anyhow::Result<serde_json::Value> {
    let mut rest_answered = false;
    for collection in ["certificates", "ca_certificates"] {
        match client.rest_json("GET", &alias_path(collection, cert_id)?, None) {
            Ok(resp) => {
                if let Some(entry) = resp.get("data").filter(|d| d.get("certificate").is_some()) {
                    return Ok(json!({"data": describe(entry, true)}));
                }
            }
            // The API is there, it just has no such alias in this collection.
            Err(e) if e.to_string().contains("Item does not exist") => rest_answered = true,
            Err(e) => debug!("Certificate REST API failed ({}), trying the legacy API", e),
        }
    }
    if rest_answered {
        anyhow::bail!("No certificate with alias '{}' (see 'vapx cert list')", cert_id);
    }
    legacy_request(
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

/// `/config/rest/cert/v1/<collection>/<alias>` with the alias percent-encoded as one path segment.
fn alias_path(collection: &str, alias: &str) -> anyhow::Result<String> {
    let mut url = url::Url::parse("http://camera.invalid")?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("cannot build a certificate path"))?
        .extend(["config", "rest", "cert", "v1", collection])
        .push(alias);
    Ok(url.path().to_string())
}

fn summarize_list(data: &serde_json::Value) -> serde_json::Value {
    let describe_all = |key: &str| -> Vec<serde_json::Value> {
        data.get(key)
            .and_then(|v| v.as_array())
            .map(|a| a.iter().map(|c| describe(c, false)).collect())
            .unwrap_or_default()
    };
    json!({
        "certificates": describe_all("certificates"),
        "ca_certificates": describe_all("ca_certificates"),
        "keystores": data.get("keystores").cloned().unwrap_or_else(|| json!([])),
        "default_keystore": data.pointer("/settings/keystore").cloned().unwrap_or(serde_json::Value::Null),
    })
}

/// One certificate entry (`alias`, `certificate` PEM, optional `keystore`) with
/// the fields read out of the certificate. `with_pem` keeps the PEM text.
fn describe(entry: &serde_json::Value, with_pem: bool) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for key in ["alias", "keystore"] {
        if let Some(v) = entry.get(key) {
            out.insert(key.into(), v.clone());
        }
    }
    let pem = entry.get("certificate").and_then(|v| v.as_str()).unwrap_or("");
    match parse_pem(pem) {
        Ok(fields) => out.extend(fields),
        Err(e) => {
            out.insert("parse_error".into(), json!(e.to_string()));
        }
    }
    if with_pem {
        out.insert("certificate".into(), json!(pem));
    }
    serde_json::Value::Object(out)
}

fn iso(t: x509_parser::time::ASN1Time) -> String {
    let d = t.to_datetime();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        d.year(),
        u8::from(d.month()),
        d.day(),
        d.hour(),
        d.minute(),
        d.second()
    )
}

/// `subject`, `issuer`, `serial`, `not_before`, `not_after` and `expired` of a PEM certificate.
fn parse_pem(pem: &str) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    let (_, block) = x509_parser::pem::parse_x509_pem(pem.as_bytes())?;
    let cert = block.parse_x509()?;
    let v = cert.validity();
    let mut m = serde_json::Map::new();
    m.insert("subject".into(), json!(cert.subject().to_string()));
    m.insert("issuer".into(), json!(cert.issuer().to_string()));
    m.insert("serial".into(), json!(cert.raw_serial_as_string()));
    m.insert("not_before".into(), json!(iso(v.not_before)));
    m.insert("not_after".into(), json!(iso(v.not_after)));
    m.insert("expired".into(), json!(!v.is_valid()));
    Ok(m)
}

/// Create a self-signed certificate.
pub fn create_self_signed(
    client: &VapixClient,
    common_name: &str,
    days: u32,
) -> anyhow::Result<serde_json::Value> {
    legacy_request(
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
    legacy_request(
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
    legacy_request(
        client,
        &json!({
            "apiVersion": "1.0",
            "method": "generateCertificateSigningRequest",
            "params": params,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const LIST: &str = include_str!("testdata/cert_v1.json");
    const ONE: &str = include_str!("testdata/cert_v1_one.json");

    fn data(raw: &str) -> Value {
        serde_json::from_str::<Value>(raw).unwrap()["data"].clone()
    }

    #[test]
    fn the_recorded_list_is_summarised_with_certificate_fields() {
        let s = summarize_list(&data(LIST));
        let certs = s["certificates"].as_array().unwrap();
        assert_eq!(certs.len(), 2);
        assert_eq!(certs[0]["alias"], "Example device ID (802.1AR)");
        assert_eq!(certs[0]["keystore"], "SE0");
        assert!(certs[0]["subject"].as_str().unwrap().contains("CN=Example Device 1"), "{}", certs[0]);
        assert!(certs[0]["issuer"].as_str().unwrap().contains("O=Example Corp"));
        assert_eq!(certs[0]["expired"], false);
        assert!(certs[0]["not_after"].as_str().unwrap().ends_with('Z'));
        assert!(certs[0].get("certificate").is_none(), "the list leaves the PEM out");
        assert_eq!(s["default_keystore"], "SE0");
        assert_eq!(s["keystores"][1]["security_level"], "STRONG");
    }

    #[test]
    fn ca_certificates_report_expiry() {
        let s = summarize_list(&data(LIST));
        let cas = s["ca_certificates"].as_array().unwrap();
        assert_eq!(cas[0]["alias"], "Example Root CA");
        assert_eq!(cas[0]["expired"], false);
        assert_eq!(cas[1]["alias"], "Example Expired CA");
        assert_eq!(cas[1]["expired"], true);
        assert_eq!(cas[1]["not_after"], "2021-01-01T00:00:00Z");
    }

    #[test]
    fn one_certificate_keeps_its_pem() {
        let d = describe(&data(ONE), true);
        assert_eq!(d["alias"], "client");
        assert!(d["certificate"].as_str().unwrap().starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(d["subject"].as_str().unwrap().contains("Example Device 2"));
    }

    #[test]
    fn an_unreadable_certificate_is_reported_not_fatal() {
        let d = describe(&json!({"alias": "broken", "certificate": "not a certificate"}), false);
        assert_eq!(d["alias"], "broken");
        assert!(d["parse_error"].is_string(), "{d}");
    }

    #[test]
    fn aliases_become_one_encoded_path_segment() {
        assert_eq!(
            alias_path("certificates", "Axis device ID ECC-P256 (802.1AR)").unwrap(),
            "/config/rest/cert/v1/certificates/Axis%20device%20ID%20ECC-P256%20(802.1AR)"
        );
        // A slash in an alias must not open a new path level.
        assert_eq!(alias_path("certificates", "a/b").unwrap(), "/config/rest/cert/v1/certificates/a%2Fb");
    }
}
