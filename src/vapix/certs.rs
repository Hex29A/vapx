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

/// Whether the camera has the certificate REST API (AXIS OS 12 and later). The write commands use it when it
/// is there and report its errors as they are; only a camera without it is asked through the legacy API.
fn rest_available(client: &VapixClient) -> bool {
    client.rest_json("GET", REST_BASE, None).is_ok()
}

/// An RFC 4514 distinguished name `CN=<common name>`, with the characters that have a meaning in a DN escaped.
fn subject_dn(attributes: &[(&str, &str)]) -> String {
    fn escape(value: &str) -> String {
        let mut out = String::new();
        let last = value.chars().count().saturating_sub(1);
        for (i, c) in value.chars().enumerate() {
            let special = matches!(c, ',' | '+' | '"' | '\\' | '<' | '>' | ';' | '=')
                || (i == 0 && matches!(c, '#' | ' '))
                || (i == last && c == ' ');
            if special {
                out.push('\\');
            }
            out.push(c);
        }
        out
    }
    attributes
        .iter()
        .map(|(k, v)| format!("{}={}", k, escape(v)))
        .collect::<Vec<_>>()
        .join(",")
}

fn check_alias(alias: &str) -> anyhow::Result<()> {
    if alias.trim().is_empty() || alias.len() > 128 || alias.chars().any(|c| c.is_control()) {
        anyhow::bail!("The certificate alias must be 1 to 128 characters without control characters");
    }
    Ok(())
}

/// Options of `create_certificate` besides the alias and the subject.
#[derive(Default)]
pub struct SelfSignOptions<'a> {
    /// Key type, for example `EC-P256` or `RSA-2048`; the camera picks one when left out.
    pub key_type: Option<&'a str>,
    /// Keystore that holds the private key; the camera's default when left out.
    pub keystore: Option<&'a str>,
}

/// Request body of `POST /config/rest/cert/v1/create_certificate`; `valid_to` is seconds since the Unix epoch.
fn create_body(alias: &str, subject: &str, valid_to: u64, opts: &SelfSignOptions) -> serde_json::Value {
    let mut data = serde_json::Map::new();
    data.insert("alias".into(), json!(alias));
    data.insert("subject".into(), json!(subject));
    data.insert("valid_to".into(), json!(valid_to));
    if let Some(k) = opts.key_type {
        data.insert("key_type".into(), json!(k));
    }
    if let Some(k) = opts.keystore {
        data.insert("keystore".into(), json!(k));
    }
    json!({"data": data})
}

/// Create a self-signed certificate. On AXIS OS 12 and later the camera makes the key pair (in `opts.keystore`
/// or its default one) and the certificate is stored under `alias`; cameras without the REST API use the legacy
/// call, which takes neither alias nor options.
pub fn create_self_signed(
    client: &VapixClient,
    common_name: &str,
    alias: &str,
    days: u32,
    opts: &SelfSignOptions,
) -> anyhow::Result<serde_json::Value> {
    if days == 0 {
        anyhow::bail!("--days must be at least 1");
    }
    check_alias(alias)?;
    if rest_available(client) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let body = create_body(alias, &subject_dn(&[("CN", common_name)]), now + u64::from(days) * 86_400, opts);
        client.rest_json("POST", &format!("{}/create_certificate", REST_BASE), Some(&body))?;
        // What the camera stored, read back, so that the output does not depend on the shape of the answer.
        return Ok(match info(client, alias) {
            Ok(resp) => resp,
            Err(_) => json!({"data": {"alias": alias}}),
        });
    }
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

/// Which collection of the REST API an alias lives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Server and client certificates (`certificates`).
    Certificate,
    /// CA certificates (`ca_certificates`).
    Ca,
}

impl Kind {
    fn collection(self) -> &'static str {
        match self {
            Kind::Certificate => "certificates",
            Kind::Ca => "ca_certificates",
        }
    }
}

/// Whether the REST API has `alias` in `kind`'s collection.
fn exists(client: &VapixClient, kind: Kind, alias: &str) -> anyhow::Result<bool> {
    match client.rest_json("GET", &alias_path(kind.collection(), alias)?, None) {
        Ok(resp) => Ok(resp.get("data").is_some_and(|d| d.get("certificate").is_some())),
        Err(e) if e.to_string().contains("Item does not exist") => Ok(false),
        Err(e) => Err(e),
    }
}

/// Remove a certificate by alias (REST) or ID (legacy). With `kind` left out, both collections are searched and
/// an alias found in both is refused, since the caller has to say which one is meant. Returns the collection
/// the certificate was removed from (`None` for the legacy API).
pub fn remove(client: &VapixClient, cert_id: &str, kind: Option<Kind>) -> anyhow::Result<Option<Kind>> {
    if rest_available(client) {
        let kind = match kind {
            Some(k) => {
                if !exists(client, k, cert_id)? {
                    anyhow::bail!("No {} with alias '{}' (see 'vapx cert list')", describe_kind(k), cert_id);
                }
                k
            }
            None => {
                let found: Vec<Kind> = [Kind::Certificate, Kind::Ca]
                    .into_iter()
                    .filter_map(|k| exists(client, k, cert_id).map(|e| e.then_some(k)).transpose())
                    .collect::<anyhow::Result<_>>()?;
                match found.as_slice() {
                    [k] => *k,
                    [] => anyhow::bail!("No certificate with alias '{}' (see 'vapx cert list')", cert_id),
                    _ => anyhow::bail!(
                        "'{}' is both a certificate and a CA certificate; nothing was removed. \
                         Say which one with --kind certificate or --kind ca",
                        cert_id
                    ),
                }
            }
        };
        client.rest_json("DELETE", &alias_path(kind.collection(), cert_id)?, None)?;
        return Ok(Some(kind));
    }
    if kind == Some(Kind::Ca) {
        anyhow::bail!("This camera has no certificate REST API, so there is no separate CA collection to remove from");
    }
    legacy_request(
        client,
        &json!({
            "apiVersion": "1.0",
            "method": "removeCertificate",
            "params": {
                "id": cert_id,
            },
        }),
    )?;
    Ok(None)
}

fn describe_kind(kind: Kind) -> &'static str {
    match kind {
        Kind::Certificate => "certificate",
        Kind::Ca => "CA certificate",
    }
}

/// Request body of `POST .../certificates/<alias>/get_csr`: the subject is only sent when something is given.
fn csr_body(common_name: Option<&str>, country: Option<&str>, organization: Option<&str>) -> serde_json::Value {
    let mut attributes = Vec::new();
    if let Some(cn) = common_name {
        attributes.push(("CN", cn));
    }
    if let Some(o) = organization {
        attributes.push(("O", o));
    }
    if let Some(c) = country {
        attributes.push(("C", c));
    }
    if attributes.is_empty() {
        json!({"data": {}})
    } else {
        json!({"data": {"subject": subject_dn(&attributes)}})
    }
}

/// Create a Certificate Signing Request (CSR).
///
/// With the REST API (AXIS OS 12 and later) the CSR is made for the key of an existing certificate, so `alias`
/// is needed (`cert self-sign` makes a certificate with a new key); the subject is the one given, or the
/// certificate's own when nothing is given. The legacy API makes a key and a CSR in one step and needs
/// `common_name`.
pub fn create_csr(
    client: &VapixClient,
    alias: Option<&str>,
    common_name: Option<&str>,
    country: Option<&str>,
    organization: Option<&str>,
) -> anyhow::Result<serde_json::Value> {
    if rest_available(client) {
        let alias = alias.ok_or_else(|| {
            anyhow::anyhow!(
                "On AXIS OS 12 and later a CSR is made for the key of an existing certificate: pass --alias \
                 (see 'vapx cert list'; 'vapx cert self-sign' creates a certificate with a new key)"
            )
        })?;
        let body = csr_body(common_name, country, organization);
        let path = format!("{}/get_csr", alias_path("certificates", alias)?);
        let resp = client.rest_json("POST", &path, Some(&body))?;
        return Ok(json!({"data": {"alias": alias, "csr": resp.get("data").cloned().unwrap_or_default()}}));
    }
    let common_name = common_name.ok_or_else(|| anyhow::anyhow!("--cn is needed for this camera"))?;
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
    #[test]
    fn a_subject_escapes_what_a_dn_gives_a_meaning() {
        assert_eq!(subject_dn(&[("CN", "camera.example")]), "CN=camera.example");
        assert_eq!(subject_dn(&[("CN", "a,b+c")]), "CN=a\\,b\\+c");
        assert_eq!(subject_dn(&[("CN", " x ")]), "CN=\\ x\\ ");
        assert_eq!(subject_dn(&[("CN", "#1")]), "CN=\\#1");
        assert_eq!(subject_dn(&[("CN", "x"), ("O", "Example, Inc."), ("C", "SE")]), "CN=x,O=Example\\, Inc.,C=SE");
    }

    #[test]
    fn create_certificate_carries_alias_subject_validity_and_only_the_options_given() {
        let body = create_body("web", "CN=camera.example", 1_800_000_000, &SelfSignOptions::default());
        assert_eq!(body, json!({"data": {"alias": "web", "subject": "CN=camera.example", "valid_to": 1_800_000_000u64}}));
        let body = create_body("web", "CN=x", 1, &SelfSignOptions { key_type: Some("EC-P256"), keystore: Some("SE0") });
        assert_eq!(body["data"]["key_type"], "EC-P256");
        assert_eq!(body["data"]["keystore"], "SE0");
    }

    #[test]
    fn a_csr_subject_is_only_sent_when_one_is_given() {
        assert_eq!(csr_body(None, None, None), json!({"data": {}}));
        assert_eq!(csr_body(Some("x"), Some("SE"), Some("Org")), json!({"data": {"subject": "CN=x,O=Org,C=SE"}}));
        assert_eq!(csr_body(None, Some("SE"), None), json!({"data": {"subject": "C=SE"}}));
    }

    #[test]
    fn aliases_with_control_characters_or_no_text_are_refused() {
        assert!(check_alias("web").is_ok());
        assert!(check_alias("").is_err());
        assert!(check_alias("  ").is_err());
        assert!(check_alias("a\nb").is_err());
        assert!(check_alias(&"x".repeat(129)).is_err());
    }
}
