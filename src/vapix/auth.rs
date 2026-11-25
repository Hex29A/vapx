use digest_auth::{AuthContext, AuthorizationHeader, HttpMethod, WwwAuthenticateHeader};
use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderValue, AUTHORIZATION, WWW_AUTHENTICATE};
use tracing::debug;

/// Build a request for any HTTP method. An unparseable method falls back to GET.
fn request_builder(client: &Client, method: &str, url: &str) -> reqwest::blocking::RequestBuilder {
    let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);
    client.request(m, url)
}

/// The request URI (path and query) that a Digest response must be computed for.
fn request_uri(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.find('/').map_or("/", |i| &rest[i..])
}

/// Perform a request with automatic Digest/Basic auth negotiation.
/// Strategy: if `https` → Basic first, and on a 401 with a Digest challenge retry with Digest. If `http` → try without auth,
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
        basic_then_digest(client, method, url, body, content_type, username, password)
    } else {
        digest_request(client, method, url, body, content_type, username, password)
    }
}

/// The Digest challenge of a 401 answer, if it carries one.
fn digest_challenge(resp: &Response) -> Option<String> {
    resp.headers()
        .get_all(WWW_AUTHENTICATE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.trim_start().to_ascii_lowercase().starts_with("digest"))
        .map(str::to_string)
}

/// HTTPS: send Basic first. A camera whose HTTP authentication policy is
/// digest-only refuses Basic even over TLS and answers 401 with a Digest
/// challenge; answer that challenge instead of returning the 401.
pub(crate) fn basic_then_digest(
    client: &Client,
    method: &str,
    url: &str,
    body: Option<&[u8]>,
    content_type: Option<&str>,
    username: &str,
    password: &str,
) -> anyhow::Result<Response> {
    debug!("Using Basic auth over HTTPS");
    let mut req = request_builder(client, method, url).basic_auth(username, Some(password));
    if let Some(ct) = content_type {
        req = req.header("Content-Type", ct);
    }
    if let Some(b) = body {
        req = req.body(b.to_vec());
    }
    let resp = req.send()?;
    if resp.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Ok(resp);
    }
    let Some(challenge) = digest_challenge(&resp) else {
        return Ok(resp);
    };
    debug!("Basic refused over HTTPS, answering the Digest challenge");
    send_digest(client, method, url, body, content_type, username, password, &challenge)
}

/// Perform a request with preemptive Digest auth for large bodies.
/// Instead of sending the full body on the initial probe, sends an empty GET
/// to obtain the digest nonce, then sends the real request with auth pre-set.
/// For HTTPS, behaves the same as `request_with_auth` (Basic first, Digest on a Digest challenge).
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
        // HTTPS: Basic first, Digest when the camera demands it
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
/// Probes with an empty GET to obtain the digest nonce. Over HTTPS, returns
/// `None` unless the camera demands Digest (digest-only policy); the caller
/// then uses `basic_auth()`.
pub fn probe_auth_header(
    client: &Client,
    method: &str,
    url: &str,
    body: Option<&[u8]>,
    username: &str,
    password: &str,
    use_https: bool,
) -> anyhow::Result<Option<String>> {
    debug!("Probing for Digest challenge");
    let probe_resp = client.get(url).send()?;

    if probe_resp.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Ok(None);
    }

    let www_auth = if use_https {
        match digest_challenge(&probe_resp) {
            Some(c) => c,
            None => return Ok(None),
        }
    } else {
        probe_resp
            .headers()
            .get(WWW_AUTHENTICATE)
            .ok_or_else(|| anyhow::anyhow!("401 without WWW-Authenticate header"))?
            .to_str()?
            .to_string()
    };
    let www_auth = www_auth.as_str();

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
    let mut req = request_builder(client, method, url);
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
        .to_str()?
        .to_string();

    send_digest(client, method, url, body, content_type, username, password, &www_auth)
}

/// Answer a Digest challenge: compute the response and send the request again.
#[allow(clippy::too_many_arguments)]
fn send_digest(
    client: &Client,
    method: &str,
    url: &str,
    body: Option<&[u8]>,
    content_type: Option<&str>,
    username: &str,
    password: &str,
    www_auth: &str,
) -> anyhow::Result<Response> {
    debug!("Got Digest challenge: {}", www_auth);

    let mut www_header = WwwAuthenticateHeader::parse(www_auth)?;

    // Extract path from URL for digest calculation
    let uri_path = request_uri(url);

    let context = AuthContext::new_with_method(
        username,
        password,
        uri_path,
        body,
        HttpMethod::from(method),
    );

    let auth_header: AuthorizationHeader = www_header.respond(&context)?;
    let auth_value = auth_header.to_header_string();

    debug!("Resending with Digest auth");

    // Resend with auth
    let mut req = request_builder(client, method, url);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_uri_axis_cgi_with_query() {
        assert_eq!(
            request_uri("http://192.0.2.10:80/axis-cgi/param.cgi?action=list&group=root.Brand"),
            "/axis-cgi/param.cgi?action=list&group=root.Brand"
        );
    }

    #[test]
    fn test_request_uri_config_rest() {
        assert_eq!(
            request_uri("https://192.0.2.10:443/config/rest/ssh/v2/users/support/password"),
            "/config/rest/ssh/v2/users/support/password"
        );
    }

    #[test]
    fn test_request_uri_without_path() {
        assert_eq!(request_uri("http://192.0.2.10:8080"), "/");
    }

    use crate::vapix::testserver::{Reply, TestServer};

    const DIGEST: &str = r#"Digest realm="AXIS", nonce="abc123", qop="auth", algorithm=MD5"#;

    /// A camera with a digest-only policy refuses Basic and answers the
    /// Digest retry. `basic_then_digest` is the HTTPS path; the fake speaks
    /// plain HTTP, which does not change the auth logic.
    #[test]
    fn https_falls_back_to_digest_when_basic_is_refused() {
        let server = TestServer::start(|req| {
            if req.to_ascii_lowercase().contains("authorization: digest") {
                Reply::Json(200, r#"{"ok":true}"#)
            } else {
                Reply::Unauthorized(DIGEST)
            }
        });
        let client = reqwest::blocking::Client::new();
        let url = format!("http://127.0.0.1:{}/axis-cgi/param.cgi?action=list", server.port);
        let resp = basic_then_digest(&client, "GET", &url, None, None, "root", "pass").unwrap();
        assert_eq!(resp.status(), 200);
        let reqs = server.requests();
        assert_eq!(reqs.len(), 2);
        assert!(reqs[0].to_ascii_lowercase().contains("authorization: basic"));
        assert!(reqs[1].to_ascii_lowercase().contains("authorization: digest"));
        assert!(reqs[1].contains("uri=\"/axis-cgi/param.cgi?action=list\""));
    }

    #[test]
    fn https_basic_is_kept_when_the_camera_accepts_it() {
        let server = TestServer::start(|_| Reply::Json(200, "{}"));
        let client = reqwest::blocking::Client::new();
        let url = format!("http://127.0.0.1:{}/x", server.port);
        let resp = basic_then_digest(&client, "GET", &url, None, None, "root", "pass").unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(server.requests().len(), 1);
    }

    #[test]
    fn https_wrong_password_is_not_retried_in_a_loop() {
        // Digest is answered once; a second 401 is returned as is.
        let server = TestServer::start(|_| Reply::Unauthorized(DIGEST));
        let client = reqwest::blocking::Client::new();
        let url = format!("http://127.0.0.1:{}/x", server.port);
        let resp = basic_then_digest(&client, "GET", &url, None, None, "root", "bad").unwrap();
        assert_eq!(resp.status(), 401);
        assert_eq!(server.requests().len(), 2);
    }

    #[test]
    fn https_basic_challenge_only_is_returned_unchanged() {
        let server = TestServer::start(|_| Reply::Unauthorized(r#"Basic realm="AXIS""#));
        let client = reqwest::blocking::Client::new();
        let url = format!("http://127.0.0.1:{}/x", server.port);
        let resp = basic_then_digest(&client, "GET", &url, None, None, "root", "pass").unwrap();
        assert_eq!(resp.status(), 401);
        assert_eq!(server.requests().len(), 1);
    }

    #[test]
    fn probe_over_https_returns_digest_only_when_demanded() {
        let digest = TestServer::start(|_| Reply::Unauthorized(DIGEST));
        let client = reqwest::blocking::Client::new();
        let url = format!("http://127.0.0.1:{}/axis-cgi/x", digest.port);
        let h = probe_auth_header(&client, "POST", &url, Some(b"x"), "root", "pass", true).unwrap();
        assert!(h.unwrap().starts_with("Digest "));
        let basic = TestServer::start(|_| Reply::Unauthorized(r#"Basic realm="AXIS""#));
        let url = format!("http://127.0.0.1:{}/axis-cgi/x", basic.port);
        assert!(probe_auth_header(&client, "POST", &url, Some(b"x"), "root", "pass", true).unwrap().is_none());
        let open = TestServer::start(|_| Reply::Json(200, "{}"));
        let url = format!("http://127.0.0.1:{}/axis-cgi/x", open.port);
        assert!(probe_auth_header(&client, "POST", &url, Some(b"x"), "root", "pass", true).unwrap().is_none());
    }
}
