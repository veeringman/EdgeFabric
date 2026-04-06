//! `ef session` — create, get, list, end sessions.

use anyhow::Result;
use serde_json::json;

use crate::client::ApiClient;
use crate::output::{OutputFormat, print_record, print_table};

const SESSION_COLUMNS: &[(&str, &str)] = &[
    ("ID", "id"),
    ("DEVICE", "device_id"),
    ("STATUS", "status"),
    ("CREATED", "created_at"),
    ("EXPIRES", "expires_at"),
];

const SESSION_DETAIL_FIELDS: &[(&str, &str)] = &[
    ("ID", "id"),
    ("Device ID", "device_id"),
    ("Tenant ID", "tenant_id"),
    ("Status", "status"),
    ("Created", "created_at"),
    ("Expires", "expires_at"),
    ("Ended", "ended_at"),
];

pub async fn list(client: &ApiClient, format: OutputFormat) -> Result<()> {
    let resp = client.get("/api/v1/sessions").await?;
    let sessions = resp["sessions"].as_array().cloned().unwrap_or_default();
    let total = resp["total"].as_u64().unwrap_or(sessions.len() as u64);

    print_table(&sessions, SESSION_COLUMNS, format);
    eprintln!("\n{total} session(s)");
    Ok(())
}

pub async fn get(client: &ApiClient, session_id: &str, format: OutputFormat) -> Result<()> {
    let resp = client.get(&format!("/api/v1/sessions/{session_id}")).await?;
    let session = &resp["session"];
    if session.is_null() {
        print_record(&resp, SESSION_DETAIL_FIELDS, format);
    } else {
        print_record(session, SESSION_DETAIL_FIELDS, format);
    }
    Ok(())
}

pub async fn create(
    client: &ApiClient,
    device_id: &str,
    duration_minutes: i64,
    format: OutputFormat,
) -> Result<()> {
    let body = json!({
        "device_id": device_id,
        "duration_minutes": duration_minutes,
    });
    let resp = client.post("/api/v1/sessions", &body).await?;
    let session = &resp["session"];

    eprintln!("Session created.");
    if session.is_null() {
        print_record(&resp, SESSION_DETAIL_FIELDS, format);
    } else {
        print_record(session, SESSION_DETAIL_FIELDS, format);
    }
    Ok(())
}

pub async fn end(client: &ApiClient, session_id: &str, format: OutputFormat) -> Result<()> {
    let resp = client
        .post(&format!("/api/v1/sessions/{session_id}/end"), &json!({}))
        .await?;
    let session = &resp["session"];

    eprintln!("Session ended.");
    if session.is_null() {
        print_record(&resp, SESSION_DETAIL_FIELDS, format);
    } else {
        print_record(session, SESSION_DETAIL_FIELDS, format);
    }
    Ok(())
}
