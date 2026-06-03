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

/// Perform a request with preemptive Digest auth for large bodies.
/// Instead of sending the full body on the initial probe, sends an empty GET
/// to obtain the digest nonce, then sends the real request with auth pre-set.
/// For HTTPS, behaves the same as `request_with_auth` (Basic auth, single send).
#[allow(clippy::too_many_arguments)]
pub fn request_with_preemptive_auth(
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
        // HTTPS uses Basic — single send, no probe needed
        return request_with_auth(client, method, url, body, content_type, username, password, use_https);
    }

    // HTTP: probe for digest challenge with empty GET, then send body once
    debug!("Probing for Digest challenge (preemptive auth)");
    let probe_resp = client.get(url).send()?;

    if probe_resp.status() != reqwest::StatusCode::UNAUTHORIZED {
        // No auth required — send the real request without auth
        debug!("No auth challenge on probe, sending without auth");
        let mut req = match method {
            "POST" => client.post(url),
            _ => client.get(url),
        };
        if let Some(ct) = content_type {
            req = req.header("Content-Type", ct);
        }
        if let Some(b) = body {
            req = req.body(b.to_vec());
        }
        return Ok(req.send()?);
    }

    let www_auth = probe_resp
        .headers()
        .get(WWW_AUTHENTICATE)
        .ok_or_else(|| anyhow::anyhow!("401 without WWW-Authenticate header"))?
        .to_str()?;

    debug!("Got Digest challenge from probe: {}", www_auth);

    let mut www_header = WwwAuthenticateHeader::parse(www_auth)?;

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

    debug!("Sending request with preemptive Digest auth");

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

/// Probe a URL for Digest auth and return the Authorization header value.
/// For HTTPS, returns `None` — caller should use `basic_auth()` directly.
/// For HTTP, probes with an empty GET to obtain the digest nonce.
pub fn probe_auth_header(
    client: &Client,
    method: &str,
    url: &str,
    body: Option<&[u8]>,
    username: &str,
    password: &str,
    use_https: bool,
) -> anyhow::Result<Option<String>> {
    if use_https {
        // Basic auth doesn't double-send; caller uses reqwest basic_auth directly
        return Ok(None);
    }

    debug!("Probing for Digest challenge");
    let probe_resp = client.get(url).send()?;

    if probe_resp.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Ok(None);
    }

    let www_auth = probe_resp
        .headers()
        .get(WWW_AUTHENTICATE)
        .ok_or_else(|| anyhow::anyhow!("401 without WWW-Authenticate header"))?
        .to_str()?;

    let mut www_header = WwwAuthenticateHeader::parse(www_auth)?;

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
    Ok(Some(auth_header.to_header_string()))
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
