//! HTTP client for the EdgeFabric REST API.

use anyhow::{Context, Result, bail};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::Value;

use crate::config::{Credentials, load_credentials};

/// Authenticated HTTP client wrapping reqwest.
pub struct ApiClient {
    inner: reqwest::Client,
    base_url: String,
}

impl ApiClient {
    /// Create a new client. Loads stored credentials for the `Authorization` header.
    pub fn new(base_url: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        if let Some(creds) = load_credentials()? {
            let val = HeaderValue::from_str(&format!("Bearer {}", creds.token))
                .context("Invalid token value")?;
            headers.insert(AUTHORIZATION, val);
        }

        let inner = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            inner,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    /// Create a client with an explicit token (used right after login).
    pub fn with_token(base_url: &str, token: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        let val = HeaderValue::from_str(&format!("Bearer {token}"))
            .context("Invalid token value")?;
        headers.insert(AUTHORIZATION, val);

        let inner = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            inner,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    // ── Public helpers ──────────────────────────────────────────

    pub async fn get(&self, path: &str) -> Result<Value> {
        let resp = self
            .inner
            .get(self.url(path))
            .send()
            .await
            .context("HTTP request failed")?;
        Self::handle_response(resp).await
    }

    pub async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let resp = self
            .inner
            .post(self.url(path))
            .json(body)
            .send()
            .await
            .context("HTTP request failed")?;
        Self::handle_response(resp).await
    }

    pub async fn put(&self, path: &str, body: &Value) -> Result<Value> {
        let resp = self
            .inner
            .put(self.url(path))
            .json(body)
            .send()
            .await
            .context("HTTP request failed")?;
        Self::handle_response(resp).await
    }

    pub async fn delete(&self, path: &str) -> Result<Value> {
        let resp = self
            .inner
            .delete(self.url(path))
            .send()
            .await
            .context("HTTP request failed")?;
        Self::handle_response(resp).await
    }

    // ── Login (unauthenticated) ─────────────────────────────────

    pub async fn login(base_url: &str, api_key: &str) -> Result<Value> {
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/api/v1/auth/login", base_url.trim_end_matches('/')))
            .json(&serde_json::json!({ "api_key": api_key }))
            .send()
            .await
            .context("Login request failed")?;
        Self::handle_response(resp).await
    }

    async fn handle_response(resp: reqwest::Response) -> Result<Value> {
        let status = resp.status();
        let body: Value = resp.json().await.unwrap_or_else(|_| Value::Null);

        if status.is_success() {
            Ok(body)
        } else {
            let msg = body["message"]
                .as_str()
                .or(body["error"].as_str())
                .unwrap_or("Unknown error");
            bail!("HTTP {status}: {msg}");
        }
    }
}

/// Ensure the user is logged in, returning stored credentials.
pub fn require_auth() -> Result<Credentials> {
    load_credentials()?
        .ok_or_else(|| anyhow::anyhow!("Not logged in. Run `ef login` first."))
}
