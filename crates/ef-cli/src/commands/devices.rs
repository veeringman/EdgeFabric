//! `ef devices` — list, show, filter devices.

use anyhow::Result;
use serde_json::Value;

use crate::client::ApiClient;
use crate::output::{OutputFormat, print_record, print_table};

const DEVICE_COLUMNS: &[(&str, &str)] = &[
    ("ID", "id"),
    ("NAME", "name"),
    ("OS", "os"),
    ("VERSION", "os_version"),
    ("KIND", "kind"),
    ("STATUS", "status"),
    ("REGION", "region"),
];

const DEVICE_DETAIL_FIELDS: &[(&str, &str)] = &[
    ("ID", "id"),
    ("Name", "name"),
    ("Serial", "serial"),
    ("OS", "os"),
    ("OS Version", "os_version"),
    ("Kind", "kind"),
    ("Status", "status"),
    ("Region", "region"),
    ("Capabilities", "capabilities"),
    ("Registered", "registered_at"),
    ("Last Heartbeat", "last_heartbeat"),
];

pub async fn list(
    client: &ApiClient,
    os: Option<&str>,
    status: Option<&str>,
    region: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let mut path = "/api/v1/devices".to_string();
    let mut params = Vec::new();
    if let Some(os) = os {
        params.push(format!("os={os}"));
    }
    if let Some(status) = status {
        params.push(format!("status={status}"));
    }
    if let Some(region) = region {
        params.push(format!("region={region}"));
    }
    if !params.is_empty() {
        path = format!("{path}?{}", params.join("&"));
    }

    let resp = client.get(&path).await?;
    let devices = resp["devices"].as_array().cloned().unwrap_or_default();
    let total = resp["total"].as_u64().unwrap_or(devices.len() as u64);

    print_table(&devices, DEVICE_COLUMNS, format);
    eprintln!("\n{total} device(s)");
    Ok(())
}

pub async fn get(client: &ApiClient, device_id: &str, format: OutputFormat) -> Result<()> {
    let resp = client.get(&format!("/api/v1/devices/{device_id}")).await?;
    let device = &resp["device"];
    if device.is_null() {
        print_record(&resp, DEVICE_DETAIL_FIELDS, format);
    } else {
        print_record(device, DEVICE_DETAIL_FIELDS, format);
    }
    Ok(())
}

pub async fn health(client: &ApiClient, device_id: &str, format: OutputFormat) -> Result<()> {
    let resp = client
        .get(&format!("/api/v1/devices/{device_id}/health"))
        .await?;
    let fields = &[
        ("Device ID", "device_id"),
        ("Status", "status"),
        ("Last Heartbeat", "last_heartbeat"),
        ("Healthy", "healthy"),
    ];
    print_record(&resp, fields, format);
    Ok(())
}
