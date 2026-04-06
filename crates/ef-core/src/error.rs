use thiserror::Error;
use uuid::Uuid;

/// Platform-wide error types.
#[derive(Debug, Error)]
pub enum EdgeFabricError {
    #[error("device not found: {0}")]
    DeviceNotFound(Uuid),

    #[error("session not found: {0}")]
    SessionNotFound(Uuid),

    #[error("no devices match the requested filter")]
    NoMatchingDevices,

    #[error("device {0} is not available")]
    DeviceUnavailable(Uuid),

    #[error("authentication failed")]
    AuthenticationFailed,

    #[error("permission denied")]
    PermissionDenied,

    #[error("operation timed out")]
    Timeout,

    #[error("internal error: {0}")]
    Internal(String),

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, EdgeFabricError>;
