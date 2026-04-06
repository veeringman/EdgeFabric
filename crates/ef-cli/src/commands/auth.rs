//! `ef login` — authenticate and store credentials.

use anyhow::Result;

use crate::client::ApiClient;
use crate::config::{Credentials, save_credentials};
use crate::output::{OutputFormat, print_value};

pub async fn login(base_url: &str, api_key: &str, format: OutputFormat) -> Result<()> {
    let resp = ApiClient::login(base_url, api_key).await?;

    let token = resp["token"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No token in login response"))?;
    let tenant_id = resp["tenant_id"]
        .as_str()
        .unwrap_or("");

    save_credentials(&Credentials {
        token: token.to_string(),
        tenant_id: tenant_id.to_string(),
    })?;

    eprintln!("Logged in successfully. Credentials saved to ~/.edgefabric/credentials");
    print_value(&resp, format);
    Ok(())
}

pub async fn logout() -> Result<()> {
    let path = crate::config::credentials_path()?;
    if path.exists() {
        std::fs::remove_file(&path)?;
        eprintln!("Logged out. Credentials removed.");
    } else {
        eprintln!("No active session.");
    }
    Ok(())
}
