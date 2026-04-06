use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Session lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Requested,
    Queued,
    Allocating,
    Active,
    Streaming,
    Testing,
    Releasing,
    Resetting,
    Completed,
    Failed,
}

/// A device session managed by EdgeFabric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub device_id: Uuid,
    pub tenant_id: Uuid,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
