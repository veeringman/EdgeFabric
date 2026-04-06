//! Immutable audit log for all device and session operations.
//!
//! Every significant action (device lifecycle, session creation, test submission,
//! configuration changes) is recorded as an [`AuditEntry`] and persisted to the
//! `audit_log` table.
//!
//! The audit log is **append-only** — entries are never updated or deleted.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// Categories of auditable actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditAction {
    // Device lifecycle
    DeviceRegistered,
    DeviceStatusChanged,
    DeviceDeleted,
    DeviceHeartbeat,

    // Session lifecycle
    SessionCreated,
    SessionTransitioned,
    SessionEnded,

    // Job lifecycle
    JobSubmitted,
    JobStarted,
    JobCompleted,
    JobCancelled,

    // Auth
    LoginSuccess,
    LoginFailed,
    ApiKeyCreated,
    ApiKeyRevoked,

    // Virtual device management
    VirtualDeviceProvisioned,
    VirtualDeviceDestroyed,
    SnapshotSaved,
    SnapshotRestored,

    // Admin
    ConfigChanged,
    Custom(String),
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Custom(s) => write!(f, "custom:{s}"),
            other => write!(f, "{other:?}"),
        }
    }
}

/// A single audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub tenant_id: Option<Uuid>,
    pub actor: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub detail: serde_json::Value,
    pub source_ip: Option<String>,
}

/// Write audit entries to PostgreSQL.
pub struct AuditLogger;

impl AuditLogger {
    /// Ensure the audit_log table exists (idempotent).
    pub async fn ensure_table(pool: &PgPool) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS audit_log (
                id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                timestamp     TIMESTAMPTZ NOT NULL DEFAULT now(),
                tenant_id     UUID,
                actor         TEXT NOT NULL,
                action        TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                resource_id   TEXT,
                detail        JSONB NOT NULL DEFAULT '{}',
                source_ip     TEXT
            )
            "#,
        )
        .execute(pool)
        .await?;

        // Index for common query patterns
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_audit_log_tenant_ts ON audit_log (tenant_id, timestamp DESC)",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_audit_log_action ON audit_log (action, timestamp DESC)",
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Record an audit entry.
    pub async fn log(
        pool: &PgPool,
        tenant_id: Option<Uuid>,
        actor: &str,
        action: AuditAction,
        resource_type: &str,
        resource_id: Option<&str>,
        detail: serde_json::Value,
        source_ip: Option<&str>,
    ) -> Result<Uuid, sqlx::Error> {
        let row = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO audit_log (tenant_id, actor, action, resource_type, resource_id, detail, source_ip)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(actor)
        .bind(action.to_string())
        .bind(resource_type)
        .bind(resource_id)
        .bind(detail)
        .bind(source_ip)
        .fetch_one(pool)
        .await?;

        tracing::debug!(
            audit_id = %row,
            action = %action,
            resource_type,
            "Audit entry recorded"
        );

        Ok(row)
    }

    /// Query audit entries for a tenant within a time range.
    pub async fn query(
        pool: &PgPool,
        tenant_id: Option<Uuid>,
        action_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AuditEntry>, sqlx::Error> {
        let rows = sqlx::query_as::<_, AuditRow>(
            r#"
            SELECT id, timestamp, tenant_id, actor, action, resource_type, resource_id, detail, source_ip
            FROM audit_log
            WHERE ($1::uuid IS NULL OR tenant_id = $1)
              AND ($2::text IS NULL OR action = $2)
            ORDER BY timestamp DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(tenant_id)
        .bind(action_filter)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Count audit entries matching a filter.
    pub async fn count(
        pool: &PgPool,
        tenant_id: Option<Uuid>,
        action_filter: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM audit_log
            WHERE ($1::uuid IS NULL OR tenant_id = $1)
              AND ($2::text IS NULL OR action = $2)
            "#,
        )
        .bind(tenant_id)
        .bind(action_filter)
        .fetch_one(pool)
        .await?;

        Ok(count)
    }
}

// Internal row type for sqlx deserialization.
#[derive(sqlx::FromRow)]
struct AuditRow {
    id: Uuid,
    timestamp: DateTime<Utc>,
    tenant_id: Option<Uuid>,
    actor: String,
    action: String,
    resource_type: String,
    resource_id: Option<String>,
    detail: serde_json::Value,
    source_ip: Option<String>,
}

impl From<AuditRow> for AuditEntry {
    fn from(r: AuditRow) -> Self {
        Self {
            id: r.id,
            timestamp: r.timestamp,
            tenant_id: r.tenant_id,
            actor: r.actor,
            action: r.action,
            resource_type: r.resource_type,
            resource_id: r.resource_id,
            detail: r.detail,
            source_ip: r.source_ip,
        }
    }
}
