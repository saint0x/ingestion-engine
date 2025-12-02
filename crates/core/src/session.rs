//! Session handling types.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

/// Session timeout duration (30 minutes of inactivity).
pub const SESSION_TIMEOUT_MINUTES: i64 = 30;

/// A user session for event correlation.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Session {
    /// Unique session ID
    pub id: Uuid,
    /// Associated tenant
    pub tenant_id: Uuid,
    /// Optional user ID (max 128 chars, same as Event.user_id)
    #[validate(length(max = 128))]
    pub user_id: Option<String>,
    /// Session start time
    pub started_at: DateTime<Utc>,
    /// Last activity time
    pub last_active_at: DateTime<Utc>,
    /// Event count in this session
    pub event_count: u64,
    /// Whether session is still active
    pub active: bool,
}

impl Session {
    /// Creates a new session.
    pub fn new(tenant_id: Uuid, user_id: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            user_id,
            started_at: now,
            last_active_at: now,
            event_count: 0,
            active: true,
        }
    }

    /// Checks if the session has timed out.
    pub fn is_timed_out(&self) -> bool {
        let timeout = Duration::minutes(SESSION_TIMEOUT_MINUTES);
        Utc::now() - self.last_active_at > timeout
    }

    /// Updates the session with a new event.
    pub fn record_event(&mut self) {
        self.last_active_at = Utc::now();
        self.event_count += 1;
    }

    /// Ends the session.
    pub fn end(&mut self) {
        self.active = false;
    }

    /// Returns the session duration.
    pub fn duration(&self) -> Duration {
        self.last_active_at - self.started_at
    }
}

/// Session info included in events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: Uuid,
    pub sequence_number: u64,
}
