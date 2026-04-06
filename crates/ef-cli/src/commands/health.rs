//! `ef health` — check API and system health.

use anyhow::Result;

use crate::client::ApiClient;
use crate::output::{OutputFormat, print_value};

pub async fn check(base_url: &str, format: OutputFormat) -> Result<()> {
    // Health endpoint is unauthenticated
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let resp = client
        .get(format!("{}/health", base_url.trim_end_matches('/')))
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().await.unwrap_or_default();
            eprintln!("API server: OK");
            print_value(&body, format);
        }
        Ok(r) => {
            eprintln!("API server: ERROR (HTTP {})", r.status());
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("API server: UNREACHABLE ({e})");
            std::process::exit(1);
        }
    }

    Ok(())
}
