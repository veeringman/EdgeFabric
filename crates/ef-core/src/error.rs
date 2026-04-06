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

    #[error("invalid session transition: {from} → {to}")]
    InvalidSessionTransition {
        from: String,
        to: String,
    },

    #[error("session limit exceeded for tenant {0}: max {1} active sessions")]
    SessionLimitExceeded(Uuid, i32),

    #[error("authentication failed")]
    AuthenticationFailed,

    #[error("permission denied")]
    PermissionDenied,

    #[error("operation timed out")]
    Timeout,

    #[error("not found: {0}")]
    NotFound(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, EdgeFabricError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_messages() {
        let id = Uuid::nil();
        assert_eq!(
            EdgeFabricError::DeviceNotFound(id).to_string(),
            format!("device not found: {id}")
        );
        assert_eq!(
            EdgeFabricError::NoMatchingDevices.to_string(),
            "no devices match the requested filter"
        );
        assert_eq!(
            EdgeFabricError::DeviceUnavailable(id).to_string(),
            format!("device {id} is not available")
        );
        assert_eq!(
            EdgeFabricError::AuthenticationFailed.to_string(),
            "authentication failed"
        );
        assert_eq!(
            EdgeFabricError::PermissionDenied.to_string(),
            "permission denied"
        );
        assert_eq!(EdgeFabricError::Timeout.to_string(), "operation timed out");
    }

    #[test]
    fn invalid_session_transition_display() {
        let err = EdgeFabricError::InvalidSessionTransition {
            from: "active".into(),
            to: "requested".into(),
        };
        assert!(err.to_string().contains("active"));
        assert!(err.to_string().contains("requested"));
    }

    #[test]
    fn session_limit_display() {
        let tenant = Uuid::nil();
        let err = EdgeFabricError::SessionLimitExceeded(tenant, 5);
        assert!(err.to_string().contains("max 5"));
    }

    #[test]
    fn not_found_display() {
        let err = EdgeFabricError::NotFound("widget xyz".into());
        assert_eq!(err.to_string(), "not found: widget xyz");
    }

    #[test]
    fn internal_error_display() {
        let err = EdgeFabricError::Internal("something broke".into());
        assert_eq!(err.to_string(), "internal error: something broke");
    }

    #[test]
    fn io_error_converts() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let err: EdgeFabricError = io_err.into();
        assert!(matches!(err, EdgeFabricError::Io(_)));
    }
}
