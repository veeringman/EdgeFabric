//! Webhook delivery for job completion notifications.

use crate::JobResult;

/// Send a webhook POST with the job result payload.
///
/// Retries up to 3 times with exponential backoff on failure.
pub async fn send(url: String, result: JobResult) {
    let client = reqwest::Client::new();
    let payload = serde_json::to_value(&result).unwrap_or_default();

    for attempt in 1..=3u32 {
        match client.post(&url).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(job_id = %result.job_id, url, "Webhook delivered");
                return;
            }
            Ok(resp) => {
                tracing::warn!(
                    job_id = %result.job_id,
                    url,
                    status = %resp.status(),
                    attempt,
                    "Webhook HTTP error"
                );
            }
            Err(e) => {
                tracing::warn!(
                    job_id = %result.job_id,
                    url,
                    attempt,
                    "Webhook request failed: {e}"
                );
            }
        }

        if attempt < 3 {
            let backoff = std::time::Duration::from_secs(2u64.pow(attempt));
            tokio::time::sleep(backoff).await;
        }
    }

    tracing::error!(job_id = %result.job_id, url, "Webhook delivery failed after 3 attempts");
}
