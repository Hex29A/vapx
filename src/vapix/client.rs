use anyhow::{bail, Context};
use serde_json::Value;
use std::time::Duration;
use tracing::{debug, trace, warn};

use crate::config::credentials::Credentials;
use crate::vapix::auth::request_with_auth;

const MAX_RETRIES: u32 = 3;
const INITIAL_BACKOFF_MS: u64 = 500;

pub struct VapixClient {
    inner: reqwest::blocking::Client,
    base_url: String,
    creds: Credentials,
}

/// Check if an error is retryable (connection/timeout errors).
fn is_retryable_error(err: &anyhow::Error) -> bool {
    if let Some(reqwest_err) = err.downcast_ref::<reqwest::Error>() {
        return reqwest_err.is_timeout() || reqwest_err.is_connect();
    }
    false
}

/// Check if an HTTP status code is retryable (5xx).
fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status.is_server_error()
}

/// Percent-encode a query parameter value (RFC 3986 unreserved characters pass through).
fn encode_value(v: &str) -> String {
    v.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                String::from(b as char)
            }
            _ => format!("%{:02X}", b),
        })
        .collect()
}

impl VapixClient {
    pub fn new(host: &str, port: u16, creds: Credentials, timeout_secs: u64) -> Self {
        let scheme = if creds.https { "https" } else { "http" };
        let inner = reqwest::blocking::ClientBuilder::new()
            .danger_accept_invalid_certs(!creds.verify_ssl)
            .timeout(Duration::from_secs(timeout_secs))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .expect("failed to build HTTP client");
        Self {
            inner,
            base_url: format!("{}://{}:{}", scheme, host, port),
            creds,
        }
    }

    /// POST JSON to a VAPIX API endpoint. Validates the response body for errors.
    /// Retries on 5xx and connection/timeout errors with exponential backoff.
    pub fn post_json(&self, path: &str, body: &Value) -> anyhow::Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let body_bytes = serde_json::to_vec(body)?;

        debug!("POST {} body={}", url, body);

        let resp = self.request_with_retry("POST", &url, Some(&body_bytes), Some("application/json"))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().unwrap_or_default();
            bail!("HTTP {}: {}", status.as_u16(), text);
        }

        let json: Value = resp.json().context("Failed to parse JSON response")?;
        trace!("Response: {}", json);

        // Check for VAPIX error in response body (HTTP 200 but error field present)
        if let Some(error) = json.get("error") {
            let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            bail!("VAPIX error {}: {}", code, message);
        }

        Ok(json)
    }

    /// GET with query params. Returns raw response.
    /// Retries on 5xx and connection/timeout errors with exponential backoff.
    pub fn get(
        &self,
        path: &str,
        params: &[(&str, &str)],
    ) -> anyhow::Result<reqwest::blocking::Response> {
        let mut url = format!("{}{}", self.base_url, path);
        if !params.is_empty() {
            let query: Vec<String> = params.iter().map(|(k, v)| format!("{}={}", k, encode_value(v))).collect();
            url = format!("{}?{}", url, query.join("&"));
        }

        debug!("GET {}", url);

        let resp = self.request_with_retry("GET", &url, None, None)?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().unwrap_or_default();
            bail!("HTTP {}: {}", status.as_u16(), text);
        }

        Ok(resp)
    }

    /// Core retry logic: attempt the request up to MAX_RETRIES times with exponential backoff.
    fn request_with_retry(
        &self,
        method: &str,
        url: &str,
        body: Option<&[u8]>,
        content_type: Option<&str>,
    ) -> anyhow::Result<reqwest::blocking::Response> {
        let mut last_err: Option<anyhow::Error> = None;

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                let backoff = Duration::from_millis(INITIAL_BACKOFF_MS * 2u64.pow(attempt - 1));
                warn!(
                    "Retry {}/{} after {}ms",
                    attempt,
                    MAX_RETRIES - 1,
                    backoff.as_millis()
                );
                std::thread::sleep(backoff);
            }

            match request_with_auth(
                &self.inner,
                method,
                url,
                body,
                content_type,
                &self.creds.user,
                &self.creds.pass,
                self.creds.https,
            ) {
                Ok(resp) if is_retryable_status(resp.status()) => {
                    let status = resp.status();
                    let text = resp.text().unwrap_or_default();
                    warn!("Server error HTTP {}: {}", status.as_u16(), text);
                    last_err = Some(anyhow::anyhow!("HTTP {}: {}", status.as_u16(), text));
                }
                Ok(resp) => return Ok(resp),
                Err(err) if is_retryable_error(&err) => {
                    warn!("Retryable error: {}", err);
                    last_err = Some(err);
                }
                Err(err) => return Err(err),
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Request failed after {} retries", MAX_RETRIES)))
    }

    /// GET and return response as text.
    pub fn get_text(&self, path: &str, params: &[(&str, &str)]) -> anyhow::Result<String> {
        Ok(self.get(path, params)?.text()?)
    }

    /// GET and return response as raw bytes.
    pub fn get_bytes(&self, path: &str, params: &[(&str, &str)]) -> anyhow::Result<Vec<u8>> {
        let bytes = self.get(path, params)?.bytes()?.to_vec();
        Ok(bytes)
    }

    #[allow(dead_code)]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}
