use digest_auth::{AuthContext, AuthorizationHeader, HttpMethod, WwwAuthenticateHeader};
use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderValue, AUTHORIZATION, WWW_AUTHENTICATE};
use tracing::debug;

/// Perform a request with automatic Digest/Basic auth negotiation.
/// Strategy: if `https` → use Basic directly. If `http` → try without auth,
/// on 401 with WWW-Authenticate: Digest, retry with computed digest.
#[allow(clippy::too_many_arguments)]
pub fn request_with_auth(
    client: &Client,
    method: &str,
    url: &str,
    body: Option<&[u8]>,
    content_type: Option<&str>,
    username: &str,
    password: &str,
    use_https: bool,
) -> anyhow::Result<Response> {
    if use_https {
        debug!("Using Basic auth over HTTPS");
        let mut req = match method {
            "POST" => client.post(url),
            "GET" => client.get(url),
            _ => client.get(url),
        };
        req = req.basic_auth(username, Some(password));
        if let Some(ct) = content_type {
            req = req.header("Content-Type", ct);
        }
        if let Some(b) = body {
            req = req.body(b.to_vec());
        }
        let resp = req.send()?;
        Ok(resp)
    } else {
        digest_request(client, method, url, body, content_type, username, password)
    }
}

/// Perform Digest auth: send initial request, parse 401 challenge, resend with digest.
fn digest_request(
    client: &Client,
    method: &str,
    url: &str,
    body: Option<&[u8]>,
    content_type: Option<&str>,
    username: &str,
    password: &str,
) -> anyhow::Result<Response> {
    // First request without auth to get the challenge
    debug!("Sending initial request to get Digest challenge");
    let mut req = match method {
        "POST" => client.post(url),
        "GET" => client.get(url),
        _ => client.get(url),
    };
    if let Some(ct) = content_type {
        req = req.header("Content-Type", ct);
    }
    if let Some(b) = body {
        req = req.body(b.to_vec());
    }
    let resp = req.send()?;

    if resp.status() != reqwest::StatusCode::UNAUTHORIZED {
        // No auth needed or different error
        return Ok(resp);
    }

    // Parse WWW-Authenticate header
    let www_auth = resp
        .headers()
        .get(WWW_AUTHENTICATE)
        .ok_or_else(|| anyhow::anyhow!("401 without WWW-Authenticate header"))?
        .to_str()?;

    debug!("Got Digest challenge: {}", www_auth);

    let mut www_header = WwwAuthenticateHeader::parse(www_auth)?;

    // Extract path from URL for digest calculation
    let uri_path = url
        .find("/axis-cgi")
        .map(|i| &url[i..])
        .unwrap_or("/");

    let context = AuthContext::new_with_method(
        username,
        password,
        uri_path,
        body,
        match method {
            "POST" => HttpMethod::POST,
            "GET" => HttpMethod::GET,
            _ => HttpMethod::GET,
        },
    );

    let auth_header: AuthorizationHeader = www_header.respond(&context)?;
    let auth_value = auth_header.to_header_string();

    debug!("Resending with Digest auth");

    // Resend with auth
    let mut req = match method {
        "POST" => client.post(url),
        "GET" => client.get(url),
        _ => client.get(url),
    };
    req = req.header(AUTHORIZATION, HeaderValue::from_str(&auth_value)?);
    if let Some(ct) = content_type {
        req = req.header("Content-Type", ct);
    }
    if let Some(b) = body {
        req = req.body(b.to_vec());
    }

    let resp = req.send()?;
    Ok(resp)
}
