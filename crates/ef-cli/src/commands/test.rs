//! `ef test` — submit jobs, check progress, cancel, list artifacts.

use anyhow::Result;
use serde_json::json;

use crate::client::ApiClient;
use crate::output::{OutputFormat, print_record, print_table, print_value};

const JOB_COLUMNS: &[(&str, &str)] = &[
    ("ID", "id"),
    ("STATUS", "status"),
    ("TESTS", "total_tests"),
    ("PASSED", "passed"),
    ("FAILED", "failed"),
    ("CREATED", "created_at"),
];

const JOB_DETAIL_FIELDS: &[(&str, &str)] = &[
    ("Job ID", "id"),
    ("Status", "status"),
    ("Test Suite", "test_suite"),
    ("Parallelism", "parallelism"),
    ("Total Tests", "total_tests"),
    ("Passed", "passed"),
    ("Failed", "failed"),
    ("Created", "created_at"),
    ("Started", "started_at"),
    ("Completed", "completed_at"),
];

const PROGRESS_FIELDS: &[(&str, &str)] = &[
    ("Job ID", "job_id"),
    ("Status", "status"),
    ("Total", "total"),
    ("Passed", "passed"),
    ("Failed", "failed"),
    ("Skipped", "skipped"),
    ("Pending", "pending"),
];

pub async fn list(client: &ApiClient, format: OutputFormat) -> Result<()> {
    let resp = client.get("/api/v1/jobs").await?;
    let jobs = resp["jobs"].as_array().cloned().unwrap_or_default();
    let total = resp["total"].as_u64().unwrap_or(jobs.len() as u64);

    print_table(&jobs, JOB_COLUMNS, format);
    eprintln!("\n{total} job(s)");
    Ok(())
}

pub async fn get(client: &ApiClient, job_id: &str, format: OutputFormat) -> Result<()> {
    let resp = client.get(&format!("/api/v1/jobs/{job_id}")).await?;
    let job = &resp["job"];
    if job.is_null() {
        print_record(&resp, JOB_DETAIL_FIELDS, format);
    } else {
        print_record(job, JOB_DETAIL_FIELDS, format);
    }

    // Also print test cases if available
    if let Some(cases) = resp["test_cases"].as_array() {
        if !cases.is_empty() {
            eprintln!("\nTest Cases:");
            let case_cols = &[
                ("NAME", "name"),
                ("STATUS", "status"),
                ("DEVICE", "device_id"),
                ("DURATION", "duration_ms"),
                ("ERROR", "error"),
            ];
            print_table(cases, case_cols, format);
        }
    }
    Ok(())
}

pub async fn submit(
    client: &ApiClient,
    test_suite: &str,
    test_names: Vec<String>,
    parallelism: i32,
    max_retries: i32,
    webhook_url: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let mut body = json!({
        "test_suite": test_suite,
        "test_names": test_names,
        "parallelism": parallelism,
        "max_retries": max_retries,
    });
    if let Some(url) = webhook_url {
        body["webhook_url"] = json!(url);
    }

    let resp = client.post("/api/v1/jobs", &body).await?;
    let job = &resp["job"];

    eprintln!("Job submitted.");
    if job.is_null() {
        print_record(&resp, JOB_DETAIL_FIELDS, format);
    } else {
        print_record(job, JOB_DETAIL_FIELDS, format);
    }
    Ok(())
}

pub async fn progress(client: &ApiClient, job_id: &str, format: OutputFormat) -> Result<()> {
    let resp = client
        .get(&format!("/api/v1/jobs/{job_id}/progress"))
        .await?;
    print_record(&resp, PROGRESS_FIELDS, format);
    Ok(())
}

pub async fn cancel(client: &ApiClient, job_id: &str, format: OutputFormat) -> Result<()> {
    let resp = client
        .post(&format!("/api/v1/jobs/{job_id}/cancel"), &json!({}))
        .await?;
    eprintln!("Job cancelled.");
    print_value(&resp, format);
    Ok(())
}

pub async fn artifacts(client: &ApiClient, job_id: &str, format: OutputFormat) -> Result<()> {
    let resp = client
        .get(&format!("/api/v1/jobs/{job_id}/artifacts"))
        .await?;
    let arts = resp["artifacts"].as_array().cloned().unwrap_or_default();

    let cols = &[
        ("ID", "id"),
        ("KIND", "kind"),
        ("PATH", "path"),
        ("SIZE", "size_bytes"),
        ("CREATED", "created_at"),
    ];
    print_table(&arts, cols, format);
    eprintln!("\n{} artifact(s)", arts.len());
    Ok(())
}

/// Watch a job until completion, printing progress updates.
pub async fn watch(client: &ApiClient, job_id: &str) -> Result<()> {
    eprintln!("Watching job {job_id} (press Ctrl+C to stop)...\n");
    loop {
        let resp = client
            .get(&format!("/api/v1/jobs/{job_id}/progress"))
            .await?;

        let status = resp["status"].as_str().unwrap_or("unknown");
        let total = resp["total"].as_i64().unwrap_or(0);
        let passed = resp["passed"].as_i64().unwrap_or(0);
        let failed = resp["failed"].as_i64().unwrap_or(0);
        let pending = resp["pending"].as_i64().unwrap_or(0);

        eprint!(
            "\r[{status}] {passed}/{total} passed, {failed} failed, {pending} pending   "
        );

        if status == "completed" || status == "failed" || status == "cancelled" {
            eprintln!();
            return Ok(());
        }

        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}
