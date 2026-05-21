use anyhow::{bail, Context};
use serde_json::Value;
use std::time::Duration;
use tracing::{debug, trace};

use crate::config::credentials::Credentials;
use crate::vapix::auth::request_with_auth;

pub struct VapixClient {
    inner: reqwest::blocking::Client,
    base_url: String,
    creds: Credentials,
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
    pub fn post_json(&self, path: &str, body: &Value) -> anyhow::Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let body_bytes = serde_json::to_vec(body)?;

        debug!("POST {} body={}", url, body);

        let resp = request_with_auth(
            &self.inner,
            "POST",
            &url,
            Some(&body_bytes),
            Some("application/json"),
            &self.creds.user,
            &self.creds.pass,
            self.creds.https,
        )?;

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
    #[allow(dead_code)]
    pub fn get(
        &self,
        path: &str,
        params: &[(&str, &str)],
    ) -> anyhow::Result<reqwest::blocking::Response> {
        let mut url = format!("{}{}", self.base_url, path);
        if !params.is_empty() {
            let query: Vec<String> = params.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
            url = format!("{}?{}", url, query.join("&"));
        }

        debug!("GET {}", url);

        let resp = request_with_auth(
            &self.inner,
            "GET",
            &url,
            None,
            None,
            &self.creds.user,
            &self.creds.pass,
            self.creds.https,
        )?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().unwrap_or_default();
            bail!("HTTP {}: {}", status.as_u16(), text);
        }

        Ok(resp)
    }

    /// GET and return response as text.
    #[allow(dead_code)]
    pub fn get_text(&self, path: &str, params: &[(&str, &str)]) -> anyhow::Result<String> {
        Ok(self.get(path, params)?.text()?)
    }

    /// GET and return response as raw bytes.
    #[allow(dead_code)]
    pub fn get_bytes(&self, path: &str, params: &[(&str, &str)]) -> anyhow::Result<Vec<u8>> {
        let bytes = self.get(path, params)?.bytes()?.to_vec();
        Ok(bytes)
    }

    #[allow(dead_code)]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}
