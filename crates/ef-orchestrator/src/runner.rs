//! Test case runner — executes a single test on a device.
//!
//! Dispatches to the correct platform driver (ADB, iOS/xcodebuild, IoT/SSH)
//! based on the device OS. Captures output, parses pass/fail, and stores
//! logs + screenshots as artifacts on disk.

use ef_control::command::adb::AdbDriver;
use ef_control::command::iot::IotDriver;
use ef_control::command::CommandOutput;
use ef_db::repo::device::DeviceRow;
use std::path::Path;

/// Configuration passed to the runner from the orchestration engine.
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// Base directory for storing artifacts (logs, screenshots, JUnit XML).
    pub artifact_dir: String,
    /// Test suite identifier — APK package/runner for Android, scheme for iOS,
    /// or binary path for IoT.
    pub test_suite: String,
}

/// Structured output from a single test case execution.
#[derive(Debug)]
pub struct TestResult {
    /// Whether the test passed.
    pub passed: bool,
    /// Captured stdout from the test framework.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
    /// Paths to artifacts written to disk (relative to artifact_dir).
    pub artifact_paths: Vec<ArtifactEntry>,
}

/// A single artifact produced during test execution.
#[derive(Debug, Clone)]
pub struct ArtifactEntry {
    pub kind: &'static str,
    pub path: String,
    pub size_bytes: u64,
}

/// Execute a single test case on the given device.
///
/// Returns `Ok(())` on pass, `Err(reason)` on failure.
pub async fn run_test_case(
    test_name: &str,
    device: &DeviceRow,
    config: &RunConfig,
) -> Result<TestResult, Box<dyn std::error::Error + Send + Sync>> {
    let serial = device
        .serial
        .as_deref()
        .ok_or_else(|| format!("device {} has no serial/address", device.name))?;
    let os = device.os.as_str();

    tracing::info!(
        test = test_name,
        device = %device.name,
        os,
        serial,
        "Executing test case"
    );

    // Ensure artifact directory exists for this test
    let test_artifact_dir = format!(
        "{}/jobs/{}/tests/{}",
        config.artifact_dir, device.id, test_name
    );
    tokio::fs::create_dir_all(&test_artifact_dir).await?;

    let result = match os {
        "android" => run_android(test_name, serial, &config.test_suite, &test_artifact_dir).await,
        "ios" => run_ios(test_name, serial, &config.test_suite, &test_artifact_dir).await,
        "iot" => run_iot(test_name, serial, &config.test_suite, &test_artifact_dir).await,
        other => Err(format!("unsupported device OS: {other}").into()),
    };

    // Always try to capture device logs, even on failure
    if let Err(e) = capture_logs(os, serial, &test_artifact_dir).await {
        tracing::warn!(test = test_name, "Failed to capture logs: {e}");
    }

    result
}

// ── Android ──────────────────────────────────────────────────────────────

/// Run an Android instrumentation test via `adb shell am instrument`.
///
/// `test_suite` format: `<test-runner>/<app-package>`
/// e.g. `androidx.test.runner.AndroidJUnitRunner/com.example.app.test`
///
/// Individual test is targeted via `-e class <test_name>`.
async fn run_android(
    test_name: &str,
    serial: &str,
    test_suite: &str,
    artifact_dir: &str,
) -> Result<TestResult, Box<dyn std::error::Error + Send + Sync>> {
    // Clear logcat before test run for clean capture
    let _ = AdbDriver::logcat_clear(serial).await;

    // Run instrumentation: adb shell am instrument -w -e class <test> <runner/package>
    let output = AdbDriver::shell(
        serial,
        &["am", "instrument", "-w", "-e", "class", test_name, test_suite],
    )
    .await?;

    let passed = parse_instrument_output(&output);

    // Capture screenshot on failure
    let mut artifacts = Vec::new();
    if !passed {
        if let Ok(png) = AdbDriver::screencap(serial).await {
            let path = format!("{}/failure.png", artifact_dir);
            tokio::fs::write(&path, &png).await?;
            artifacts.push(ArtifactEntry {
                kind: "screenshot",
                path,
                size_bytes: png.len() as u64,
            });
        }
    }

    // Save raw output
    let stdout_path = format!("{}/stdout.txt", artifact_dir);
    tokio::fs::write(&stdout_path, &output.stdout).await?;
    let stdout_size = output.stdout.len() as u64;
    artifacts.push(ArtifactEntry {
        kind: "log",
        path: stdout_path,
        size_bytes: stdout_size,
    });

    if !output.stderr.is_empty() {
        let stderr_path = format!("{}/stderr.txt", artifact_dir);
        let stderr_size = output.stderr.len() as u64;
        tokio::fs::write(&stderr_path, &output.stderr).await?;
        artifacts.push(ArtifactEntry {
            kind: "log",
            path: stderr_path,
            size_bytes: stderr_size,
        });
    }

    if !passed {
        return Err(format!(
            "Test {} failed: {}",
            test_name,
            extract_failure_message(&output.stdout)
        )
        .into());
    }

    Ok(TestResult {
        passed,
        stdout: output.stdout,
        stderr: output.stderr,
        artifact_paths: artifacts,
    })
}

/// Parse `am instrument` output to determine pass/fail.
///
/// Succeeds when output contains "OK (1 test)" or "OK (" and exit code is 0.
/// Fails on "FAILURES!!!", "Error", or non-zero exit code.
fn parse_instrument_output(output: &CommandOutput) -> bool {
    if output.exit_code != 0 {
        return false;
    }
    let stdout = &output.stdout;
    // Android instrumentation reports "OK (N test)" on success
    if stdout.contains("OK (") && !stdout.contains("FAILURES") {
        return true;
    }
    // Also accept INSTRUMENTATION_STATUS_CODE: 0 without failures
    if stdout.contains("INSTRUMENTATION_STATUS_CODE: 0") && !stdout.contains("FAILURES") {
        return true;
    }
    false
}

// ── iOS ──────────────────────────────────────────────────────────────────

/// Run an iOS test via `xcodebuild test-without-building`.
///
/// `test_suite` format: `<scheme>` (Xcode scheme name)
/// The specific test is targeted via `-only-testing:<test_name>`.
async fn run_ios(
    test_name: &str,
    udid: &str,
    test_suite: &str,
    artifact_dir: &str,
) -> Result<TestResult, Box<dyn std::error::Error + Send + Sync>> {
    let result_bundle = format!("{}/result.xcresult", artifact_dir);

    let output = tokio::process::Command::new("xcodebuild")
        .args([
            "test-without-building",
            "-scheme",
            test_suite,
            "-destination",
            &format!("id={udid}"),
            &format!("-only-testing:{test_name}"),
            "-resultBundlePath",
            &result_bundle,
        ])
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let passed = output.status.success() && stdout.contains("** TEST SUCCEEDED **");

    let mut artifacts = Vec::new();

    // Save stdout/stderr
    let stdout_path = format!("{}/stdout.txt", artifact_dir);
    tokio::fs::write(&stdout_path, &stdout).await?;
    artifacts.push(ArtifactEntry {
        kind: "log",
        path: stdout_path,
        size_bytes: stdout.len() as u64,
    });

    // Mark xcresult bundle as artifact if it exists
    if Path::new(&result_bundle).exists() {
        let meta = tokio::fs::metadata(&result_bundle).await.unwrap_or_else(|_| {
            std::fs::metadata(&result_bundle).unwrap()
        });
        artifacts.push(ArtifactEntry {
            kind: "junit_xml",
            path: result_bundle,
            size_bytes: meta.len(),
        });
    }

    if !passed {
        return Err(format!(
            "Test {} failed: {}",
            test_name,
            extract_failure_message(&stdout)
        )
        .into());
    }

    Ok(TestResult {
        passed,
        stdout,
        stderr,
        artifact_paths: artifacts,
    })
}

// ── IoT / SSH ────────────────────────────────────────────────────────────

/// Run a test on an IoT device via SSH.
///
/// `test_suite` format: remote binary path, e.g. `/opt/tests/runner`
/// Invokes `<test_suite> --test <test_name> --format tap` on the device.
async fn run_iot(
    test_name: &str,
    host: &str,
    test_suite: &str,
    artifact_dir: &str,
) -> Result<TestResult, Box<dyn std::error::Error + Send + Sync>> {
    let output = IotDriver::ssh_exec(
        host,
        "root",
        &[test_suite, "--test", test_name, "--format", "tap"],
    )
    .await?;

    let passed = parse_tap_output(&output.stdout);

    let mut artifacts = Vec::new();

    // Save output
    let stdout_path = format!("{}/stdout.txt", artifact_dir);
    tokio::fs::write(&stdout_path, &output.stdout).await?;
    artifacts.push(ArtifactEntry {
        kind: "log",
        path: stdout_path,
        size_bytes: output.stdout.len() as u64,
    });

    if !passed {
        return Err(format!(
            "Test {} failed: {}",
            test_name,
            extract_failure_message(&output.stdout)
        )
        .into());
    }

    Ok(TestResult {
        passed,
        stdout: output.stdout,
        stderr: output.stderr,
        artifact_paths: artifacts,
    })
}

/// Parse TAP (Test Anything Protocol) output for pass/fail.
///
/// TAP format:
/// ```text
/// ok 1 - test_name
/// not ok 2 - another_test
/// ```
fn parse_tap_output(stdout: &str) -> bool {
    // If any line starts with "not ok", the test failed
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("not ok") {
            return false;
        }
    }
    // Must have at least one "ok" line to count as passed
    stdout.lines().any(|l| l.trim().starts_with("ok "))
}

// ── Shared Helpers ───────────────────────────────────────────────────────

/// Capture device logs and write to the artifact directory.
async fn capture_logs(
    os: &str,
    serial: &str,
    artifact_dir: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let logs = match os {
        "android" => AdbDriver::logcat(serial, 500).await?.stdout,
        "ios" => {
            // idevicesyslog — capture last 500 lines
            let output = tokio::process::Command::new("idevicesyslog")
                .args(["-u", serial, "-n"])
                .output()
                .await;
            match output {
                Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
                Err(_) => String::new(),
            }
        }
        "iot" => {
            IotDriver::journal_logs(serial, "root", 500)
                .await
                .map(|o| o.stdout)
                .unwrap_or_default()
        }
        _ => String::new(),
    };

    if !logs.is_empty() {
        let path = format!("{}/device.log", artifact_dir);
        tokio::fs::write(&path, &logs).await?;
    }

    Ok(())
}

/// Extract a concise failure message from test output.
fn extract_failure_message(stdout: &str) -> String {
    // Look for common failure patterns
    for line in stdout.lines().rev() {
        let trimmed = line.trim();
        // Android instrumentation failures
        if trimmed.starts_with("FAILURES") || trimmed.starts_with("Error") {
            return trimmed.to_string();
        }
        // iOS test failures
        if trimmed.contains("failed") && trimmed.contains("(") {
            return trimmed.to_string();
        }
        // TAP "not ok" line
        if trimmed.starts_with("not ok") {
            return trimmed.to_string();
        }
    }
    // Fallback: last non-empty line
    stdout
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("unknown error")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_pass_output() {
        let output = CommandOutput {
            stdout: "INSTRUMENTATION_STATUS: test=exampleTest\nINSTRUMENTATION_STATUS_CODE: 0\nOK (1 test)".into(),
            stderr: String::new(),
            exit_code: 0,
        };
        assert!(parse_instrument_output(&output));
    }

    #[test]
    fn android_fail_output() {
        let output = CommandOutput {
            stdout: "FAILURES!!!\nTests run: 1, Failures: 1".into(),
            stderr: String::new(),
            exit_code: 1,
        };
        assert!(!parse_instrument_output(&output));
    }

    #[test]
    fn tap_pass() {
        assert!(parse_tap_output("ok 1 - test_network_connectivity\n"));
    }

    #[test]
    fn tap_fail() {
        assert!(!parse_tap_output("not ok 1 - test_network_connectivity\n"));
    }

    #[test]
    fn tap_empty_is_fail() {
        assert!(!parse_tap_output(""));
    }

    #[test]
    fn extract_failure_from_android() {
        let output = "INSTRUMENTATION_STATUS: test=exampleTest\nFAILURES!!!\nTests run: 1";
        assert_eq!(extract_failure_message(output), "FAILURES!!!");
    }
}
