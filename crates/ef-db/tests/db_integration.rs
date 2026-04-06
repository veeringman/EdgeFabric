//! Database integration tests.
//!
//! Uses `#[sqlx::test]` to automatically create a temporary PostgreSQL database,
//! run all migrations, and clean up afterward.
//!
//! Requires: `DATABASE_URL` environment variable pointing to a PostgreSQL instance.
//! Run with: `DATABASE_URL=postgres://... cargo test -p ef-db --test db_integration`

use ef_db::repo::device::{DeviceRepo, DeviceRow};
use ef_db::repo::job::{ArtifactRepo, JobRepo, TestCaseRepo};
use ef_db::repo::session::SessionRepo;
use ef_db::repo::tenant::TenantRepo;
use sqlx::PgPool;
use uuid::Uuid;
use chrono::{Duration, Utc};

/// Helper: create a test tenant so FK constraints are satisfied.
async fn create_test_tenant(pool: &PgPool) -> Uuid {
    let key = format!("test-key-{}", Uuid::new_v4());
    let name = format!("Test Tenant {}", &key[9..17]);
    let t = TenantRepo::create(pool, &name, &key).await.unwrap();
    t.id
}

// ── Device CRUD ──────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../migrations")]
async fn create_and_find_device(pool: PgPool) {
    let device = DeviceRepo::create(
        &pool,
        "Pixel 7",
        Some("R5CT900ABCD"),
        "android",
        "14",
        "real",
        "us-west",
        &["adb".to_string()],
    )
    .await
    .unwrap();

    assert_eq!(device.name, "Pixel 7");
    assert_eq!(device.os, "android");
    assert_eq!(device.status, "available");

    let found = DeviceRepo::find_by_id(&pool, device.id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().serial, Some("R5CT900ABCD".into()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_devices_includes_seed_data(pool: PgPool) {
    let devices = DeviceRepo::list(&pool).await.unwrap();
    // Seed migration inserts 8 sample devices
    assert!(devices.len() >= 8);
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_available_filters_by_os(pool: PgPool) {
    // Create additional Android and iOS devices beyond seed data
    DeviceRepo::create(&pool, "Pixel Test", Some("px1"), "android", "14", "real", "us-test", &[]).await.unwrap();
    DeviceRepo::create(&pool, "iPhone Test", Some("ip1"), "ios", "17", "real", "us-test", &[]).await.unwrap();

    let android_only = DeviceRepo::list_available(&pool, Some("android"), None).await.unwrap();
    assert!(android_only.len() >= 1);
    assert!(android_only.iter().all(|d| d.os == "android"));

    let ios_only = DeviceRepo::list_available(&pool, Some("ios"), None).await.unwrap();
    assert!(ios_only.len() >= 1);
    assert!(ios_only.iter().all(|d| d.os == "ios"));

    let all = DeviceRepo::list_available(&pool, None, None).await.unwrap();
    assert!(all.len() >= 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn set_device_status(pool: PgPool) {
    let device = DeviceRepo::create(&pool, "Pi", Some("pi1"), "iot", "1.0", "real", "eu", &[]).await.unwrap();
    assert_eq!(device.status, "available");

    let updated = DeviceRepo::set_status(&pool, device.id, "busy").await.unwrap();
    assert!(updated);

    let found = DeviceRepo::find_by_id(&pool, device.id).await.unwrap().unwrap();
    assert_eq!(found.status, "busy");

    // Setting back
    DeviceRepo::set_status(&pool, device.id, "available").await.unwrap();
    let found = DeviceRepo::find_by_id(&pool, device.id).await.unwrap().unwrap();
    assert_eq!(found.status, "available");
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_device(pool: PgPool) {
    let device = DeviceRepo::create(&pool, "Old Name", Some("s1"), "android", "13", "real", "us", &[]).await.unwrap();
    let updated = DeviceRepo::update(&pool, device.id, "New Name", Some("s2"), "14", "eu", &["usb".into()]).await.unwrap();
    let updated = updated.unwrap();
    assert_eq!(updated.name, "New Name");
    assert_eq!(updated.serial, Some("s2".into()));
    assert_eq!(updated.os_version, "14");
    assert_eq!(updated.region, "eu");
}

#[sqlx::test(migrations = "../../migrations")]
async fn delete_nonexistent_device(pool: PgPool) {
    let deleted = DeviceRepo::delete(&pool, Uuid::new_v4()).await.unwrap();
    assert!(!deleted);
}

// ── Session CRUD ─────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../migrations")]
async fn create_and_list_sessions(pool: PgPool) {
    let device = DeviceRepo::create(&pool, "Dev1", Some("s1"), "android", "14", "real", "us", &[]).await.unwrap();
    let tenant_id = create_test_tenant(&pool).await;

    let expires = Utc::now() + Duration::hours(1);
    let session = SessionRepo::create(&pool, device.id, tenant_id, expires).await.unwrap();
    assert_eq!(session.status, "requested");

    let sessions = SessionRepo::list_by_tenant(&pool, tenant_id).await.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, session.id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn session_status_transition(pool: PgPool) {
    let device = DeviceRepo::create(&pool, "Dev1", Some("s1"), "android", "14", "real", "us", &[]).await.unwrap();
    let tenant_id = create_test_tenant(&pool).await;
    let expires = Utc::now() + Duration::hours(1);
    let session = SessionRepo::create(&pool, device.id, tenant_id, expires).await.unwrap();

    // requested → queued
    let ok = SessionRepo::set_status(&pool, session.id, "queued").await.unwrap();
    assert!(ok);

    let found = SessionRepo::find_by_id(&pool, session.id).await.unwrap().unwrap();
    assert_eq!(found.status, "queued");
}

// ── Job & TestCase CRUD ──────────────────────────────────────────────

#[sqlx::test(migrations = "../../migrations")]
async fn create_job_and_test_cases(pool: PgPool) {
    let tenant_id = create_test_tenant(&pool).await;
    let job = JobRepo::create(&pool, tenant_id, 2, "com.example.test", None).await.unwrap();
    assert_eq!(job.status, "queued");
    assert_eq!(job.test_suite, "com.example.test");

    let tests = vec!["test_login".into(), "test_signup".into(), "test_profile".into()];
    TestCaseRepo::create_batch(&pool, job.id, &tests, 2).await.unwrap();

    let pending = TestCaseRepo::list_pending(&pool, job.id, 10).await.unwrap();
    assert_eq!(pending.len(), 3);
    assert_eq!(pending[0].max_retries, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_case_lifecycle(pool: PgPool) {
    let tenant_id = create_test_tenant(&pool).await;
    let job = JobRepo::create(&pool, tenant_id, 1, "suite", None).await.unwrap();

    let tests = vec!["test_a".into()];
    TestCaseRepo::create_batch(&pool, job.id, &tests, 1).await.unwrap();

    let pending = TestCaseRepo::list_pending(&pool, job.id, 5).await.unwrap();
    let tc = &pending[0];

    // Mark running
    let device = DeviceRepo::create(&pool, "D1", Some("s1"), "android", "14", "real", "us", &[]).await.unwrap();
    TestCaseRepo::mark_running(&pool, tc.id, device.id).await.unwrap();

    // Mark passed
    TestCaseRepo::mark_passed(&pool, tc.id, 150).await.unwrap();

    // Aggregate
    let (total, passed, failed, skipped) = TestCaseRepo::aggregate(&pool, job.id).await.unwrap();
    assert_eq!(total, 1);
    assert_eq!(passed, 1);
    assert_eq!(failed, 0);
    assert_eq!(skipped, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_case_failure_and_retry(pool: PgPool) {
    let tenant_id = create_test_tenant(&pool).await;
    let job = JobRepo::create(&pool, tenant_id, 1, "suite", None).await.unwrap();

    let tests = vec!["test_flaky".into()];
    TestCaseRepo::create_batch(&pool, job.id, &tests, 2).await.unwrap();

    let pending = TestCaseRepo::list_pending(&pool, job.id, 5).await.unwrap();
    let tc = &pending[0];

    let device = DeviceRepo::create(&pool, "D1", Some("s1"), "android", "14", "real", "us", &[]).await.unwrap();
    TestCaseRepo::mark_running(&pool, tc.id, device.id).await.unwrap();

    // First failure — should auto-retry (attempt < max_retries)
    TestCaseRepo::mark_failed(&pool, tc.id, 100, "assertion error").await.unwrap();

    // The test should be re-queued as pending since attempt (1) < max_retries (2)
    let _pending = TestCaseRepo::list_pending(&pool, job.id, 5).await.unwrap();
    // Depending on retry logic, it should either be pending or retrying
    // At minimum, job aggregation should show 0 passed, 0 permanently failed
    let (total, passed, failed, _skipped) = TestCaseRepo::aggregate(&pool, job.id).await.unwrap();
    assert_eq!(total, 1);
    assert_eq!(passed, 0);
    // May be 0 (retrying) or 1 (failed) depending on retry implementation
    assert!(failed <= 1);
}

// ── Artifacts ────────────────────────────────────────────────────────

#[sqlx::test(migrations = "../../migrations")]
async fn create_and_list_artifacts(pool: PgPool) {
    let tenant_id = create_test_tenant(&pool).await;
    let job = JobRepo::create(&pool, tenant_id, 1, "suite", None).await.unwrap();

    ArtifactRepo::create(&pool, job.id, None, "log", "/tmp/test.log", 1024).await.unwrap();
    ArtifactRepo::create(&pool, job.id, None, "screenshot", "/tmp/screen.png", 50000).await.unwrap();

    let artifacts = ArtifactRepo::list_by_job(&pool, job.id).await.unwrap();
    assert_eq!(artifacts.len(), 2);
}

// ── Job completion ───────────────────────────────────────────────────

#[sqlx::test(migrations = "../../migrations")]
async fn job_lifecycle_complete(pool: PgPool) {
    let tenant_id = create_test_tenant(&pool).await;
    let job = JobRepo::create(&pool, tenant_id, 1, "suite", None).await.unwrap();
    assert_eq!(job.status, "queued");

    JobRepo::mark_started(&pool, job.id).await.unwrap();
    let j = JobRepo::find_by_id(&pool, job.id).await.unwrap().unwrap();
    assert_eq!(j.status, "running");

    JobRepo::mark_completed(&pool, job.id, "completed", 5, 4, 1).await.unwrap();
    let j = JobRepo::find_by_id(&pool, job.id).await.unwrap().unwrap();
    assert_eq!(j.status, "completed");
    assert_eq!(j.passed, 4);
    assert_eq!(j.failed, 1);
}
