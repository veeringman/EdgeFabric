use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Session lifecycle states.
///
/// State machine transitions:
/// ```text
///   Requested → Queued → Allocating → Active
///                                       ↓
///                                    Streaming ↔ Testing
///                                       ↓
///                                    Releasing → Resetting → Completed
///                                       
///   (any non-terminal) ──────────────→ Failed
/// ```
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

impl SessionStatus {
    /// Parse from lowercase string (matching DB enum values).
    pub fn from_str_lowercase(s: &str) -> Option<Self> {
        match s {
            "requested" => Some(Self::Requested),
            "queued" => Some(Self::Queued),
            "allocating" => Some(Self::Allocating),
            "active" => Some(Self::Active),
            "streaming" => Some(Self::Streaming),
            "testing" => Some(Self::Testing),
            "releasing" => Some(Self::Releasing),
            "resetting" => Some(Self::Resetting),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Queued => "queued",
            Self::Allocating => "allocating",
            Self::Active => "active",
            Self::Streaming => "streaming",
            Self::Testing => "testing",
            Self::Releasing => "releasing",
            Self::Resetting => "resetting",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    /// Returns true if this is a terminal state (no further transitions allowed except → Failed).
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }

    /// Check if a transition from `self` → `target` is allowed.
    pub fn can_transition_to(&self, target: Self) -> bool {
        // Any non-terminal state can transition to Failed.
        if target == Self::Failed && !self.is_terminal() {
            return true;
        }

        matches!(
            (self, target),
            (Self::Requested, Self::Queued)
                | (Self::Queued, Self::Allocating)
                | (Self::Allocating, Self::Active)
                | (Self::Active, Self::Streaming)
                | (Self::Active, Self::Testing)
                | (Self::Active, Self::Releasing)
                | (Self::Streaming, Self::Testing)
                | (Self::Streaming, Self::Releasing)
                | (Self::Testing, Self::Streaming)
                | (Self::Testing, Self::Releasing)
                | (Self::Releasing, Self::Resetting)
                | (Self::Resetting, Self::Completed)
        )
    }
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
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
    pub ended_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_transitions() {
        let path = [
            SessionStatus::Requested,
            SessionStatus::Queued,
            SessionStatus::Allocating,
            SessionStatus::Active,
            SessionStatus::Streaming,
            SessionStatus::Testing,
            SessionStatus::Releasing,
            SessionStatus::Resetting,
            SessionStatus::Completed,
        ];
        for pair in path.windows(2) {
            assert!(
                pair[0].can_transition_to(pair[1]),
                "{:?} → {:?} should be allowed",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn fail_from_any_non_terminal() {
        let non_terminal = [
            SessionStatus::Requested,
            SessionStatus::Queued,
            SessionStatus::Allocating,
            SessionStatus::Active,
            SessionStatus::Streaming,
            SessionStatus::Testing,
            SessionStatus::Releasing,
            SessionStatus::Resetting,
        ];
        for s in &non_terminal {
            assert!(s.can_transition_to(SessionStatus::Failed), "{s:?} → Failed should be allowed");
        }
    }

    #[test]
    fn reject_invalid_transitions() {
        assert!(!SessionStatus::Completed.can_transition_to(SessionStatus::Active));
        assert!(!SessionStatus::Failed.can_transition_to(SessionStatus::Active));
        assert!(!SessionStatus::Requested.can_transition_to(SessionStatus::Active));
        assert!(!SessionStatus::Active.can_transition_to(SessionStatus::Requested));
    }

    #[test]
    fn roundtrip_str() {
        for s in [
            SessionStatus::Requested,
            SessionStatus::Active,
            SessionStatus::Completed,
            SessionStatus::Failed,
        ] {
            assert_eq!(SessionStatus::from_str_lowercase(s.as_str()), Some(s));
        }
    }
}
