//! Event type definitions for the ingestion engine.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::{Validate, ValidationError};

use crate::limits::MAX_CUSTOM_PROPERTIES_BYTES;

/// Coordinates for click events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coordinates {
    pub x: f64,
    pub y: f64,
}

/// Performance metrics for web vitals.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PerformanceMetrics {
    /// Largest Contentful Paint (ms)
    #[validate(range(min = 0.0, max = 60000.0))]
    pub lcp: Option<f64>,
    /// First Input Delay (ms)
    #[validate(range(min = 0.0, max = 10000.0))]
    pub fid: Option<f64>,
    /// Cumulative Layout Shift
    #[validate(range(min = 0.0, max = 10.0))]
    pub cls: Option<f64>,
    /// Time to First Byte (ms)
    #[validate(range(min = 0.0, max = 60000.0))]
    pub ttfb: Option<f64>,
    /// First Contentful Paint (ms)
    #[validate(range(min = 0.0, max = 60000.0))]
    pub fcp: Option<f64>,
    /// DOM Content Loaded (ms)
    #[validate(range(min = 0.0, max = 120000.0))]
    pub dom_content_loaded: Option<f64>,
    /// Load Complete (ms)
    #[validate(range(min = 0.0, max = 300000.0))]
    pub load_complete: Option<f64>,
    /// Resource count
    #[validate(range(max = 10000))]
    pub resource_count: Option<u32>,
    /// Memory usage (bytes)
    pub memory_usage: Option<u64>,
}

/// Scroll direction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScrollDirection {
    Up,
    Down,
}

/// Pageview event data.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PageviewData {
    #[validate(length(max = 500))]
    pub title: String,
    #[validate(length(max = 2000))]
    pub path: String,
    /// Page load time (ms)
    pub load_time: Option<f64>,
    /// Time to first byte (ms)
    pub time_to_first_byte: Option<f64>,
    /// Referrer URL
    #[validate(length(max = 2048))]
    pub referrer: Option<String>,
}

/// Click event data.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ClickData {
    /// Element tag name
    #[validate(length(max = 64))]
    pub element: String,
    /// CSS selector
    #[validate(length(max = 1000))]
    pub selector: Option<String>,
    /// Click coordinates
    pub coordinates: Option<Coordinates>,
    /// Whether this was a double-click
    #[serde(default)]
    pub is_double_click: bool,
    /// Element text content (truncated)
    #[validate(length(max = 200))]
    pub text: Option<String>,
}

/// Scroll event data.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ScrollData {
    /// Scroll depth as percentage (0-100)
    #[validate(range(min = 0.0, max = 100.0))]
    pub scroll_depth: f64,
    /// Scroll direction
    pub direction: ScrollDirection,
    /// Element being scrolled (if not document)
    #[validate(length(max = 256))]
    pub element: Option<String>,
}

/// Performance event data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceData {
    pub metrics: PerformanceMetrics,
}

/// Validates custom properties JSON size.
fn validate_properties_size(props: &serde_json::Value) -> Result<(), ValidationError> {
    // Fast path: null/empty
    if props.is_null() {
        return Ok(());
    }

    let size = serde_json::to_vec(props).map(|v| v.len()).unwrap_or(0);

    if size > MAX_CUSTOM_PROPERTIES_BYTES {
        let mut err = ValidationError::new("properties_too_large");
        err.message = Some(
            format!(
                "properties {}KB exceeds {}KB limit",
                size / 1024,
                MAX_CUSTOM_PROPERTIES_BYTES / 1024
            )
            .into(),
        );
        return Err(err);
    }
    Ok(())
}

/// Custom event data.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CustomData {
    /// Custom event type name
    #[validate(length(min = 1, max = 100))]
    pub event_name: String,
    /// Arbitrary properties (max 16KB)
    #[validate(custom(function = "validate_properties_size"))]
    pub properties: serde_json::Value,
}

// =============================================================================
// Overwatch Triggers v1.0 - Context-based notification system
// =============================================================================

/// Position coordinates for mouse tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

/// Mouse move event data (enhanced for Overwatch Triggers).
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MouseMoveData {
    /// Current mouse position
    pub position: Position,
    /// Mouse velocity in pixels per second
    #[validate(range(min = 0.0))]
    pub velocity: f64,
    /// Whether exit intent was detected (mouse moving toward browser chrome)
    #[serde(default)]
    pub exit_intent: bool,
    /// CSS selector of element being hovered (if any)
    #[validate(length(max = 1000))]
    pub hover_target: Option<String>,
}

/// Exit intent event data.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ExitIntentData {
    /// Mouse position when exit intent was detected
    pub position: Position,
    /// Mouse velocity in pixels per second
    #[validate(range(min = 0.0))]
    pub velocity: f64,
    /// Time spent on page in milliseconds
    #[validate(range(min = 0))]
    pub time_on_page: i64,
    /// Current scroll depth percentage (0-100)
    #[validate(range(min = 0.0, max = 100.0))]
    pub scroll_depth: f64,
}

/// Activity type for idle detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityType {
    Mouse,
    Keyboard,
    Scroll,
    Click,
    Touch,
}

/// Idle start event data.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct IdleStartData {
    /// Type of last activity before going idle
    pub last_activity_type: ActivityType,
    /// Time spent on page in milliseconds
    #[validate(range(min = 0))]
    pub time_on_page: i64,
}

/// Idle end event data.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct IdleEndData {
    /// Duration of idle period in milliseconds
    #[validate(range(min = 0))]
    pub idle_duration: i64,
    /// Type of activity that ended the idle period
    pub resume_activity_type: ActivityType,
    /// Time spent on page in milliseconds
    #[validate(range(min = 0))]
    pub time_on_page: i64,
}

/// Engagement factors for scoring.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct EngagementFactors {
    /// Time spent on page in milliseconds
    #[validate(range(min = 0))]
    pub time_on_page: i64,
    /// Current scroll depth percentage (0-100)
    #[validate(range(min = 0.0, max = 100.0))]
    pub scroll_depth: f64,
    /// Number of clicks during session
    #[validate(range(min = 0))]
    pub click_count: u32,
    /// Whether user has interacted with a form
    #[serde(default)]
    pub form_interaction: bool,
    /// Total mouse movement in pixels
    #[validate(range(min = 0.0))]
    pub mouse_activity: f64,
    /// Time page has been in focus in milliseconds
    #[validate(range(min = 0))]
    pub focus_time: i64,
}

/// Engagement snapshot event data (periodic).
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct EngagementSnapshotData {
    /// Computed engagement score (0-100)
    #[validate(range(min = 0.0, max = 100.0))]
    pub score: f64,
    /// Individual factors contributing to score
    pub factors: EngagementFactors,
}

/// Trigger fire frequency constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TriggerOnce {
    /// Fire once per page view
    Page,
    /// Fire once per session
    Session,
    /// Fire once per user (persistent)
    User,
    /// No constraint (can fire multiple times)
    #[serde(rename = "false")]
    False,
}

/// Trigger registered event data.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TriggerRegisteredData {
    /// Unique identifier for this trigger
    #[validate(length(min = 1, max = 128))]
    pub trigger_id: String,
    /// Serialized condition type (e.g., "scroll_depth>50", "idle_time>30000")
    #[validate(length(max = 1000))]
    pub condition: String,
    /// Priority for trigger ordering (0-1000, higher = more important)
    #[validate(range(min = 0, max = 1000))]
    pub priority: u32,
    /// Fire frequency constraint
    pub once: TriggerOnce,
}

/// Trigger context at fire time.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TriggerContext {
    /// Time on page when trigger fired (ms)
    #[validate(range(min = 0))]
    pub time_on_page: i64,
    /// Scroll depth when trigger fired (0-100)
    #[validate(range(min = 0.0, max = 100.0))]
    pub scroll_depth: f64,
    /// Engagement score when trigger fired (0-100)
    #[validate(range(min = 0.0, max = 100.0))]
    pub engagement_score: f64,
    /// Total session duration (ms)
    #[validate(range(min = 0))]
    pub session_duration: i64,
    /// Number of pages viewed in session
    #[validate(range(min = 0))]
    pub page_count: u32,
}

/// Trigger fired event data.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TriggerFiredData {
    /// Trigger that fired
    #[validate(length(min = 1, max = 128))]
    pub trigger_id: String,
    /// Condition that was met
    #[validate(length(max = 1000))]
    pub condition: String,
    /// Priority of the fired trigger
    #[validate(range(min = 0, max = 1000))]
    pub priority: u32,
    /// Context at the moment of firing
    pub context: TriggerContext,
}

/// Dismiss method for triggers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DismissMethod {
    /// User explicitly dismissed
    User,
    /// User snoozed (will reappear later)
    Snooze,
    /// System limit reached (too many triggers)
    Limit,
}

/// Trigger dismissed event data.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TriggerDismissedData {
    /// Trigger that was dismissed
    #[validate(length(min = 1, max = 128))]
    pub trigger_id: String,
    /// How the trigger was dismissed
    pub method: DismissMethod,
    /// Snooze duration if method is Snooze (ms)
    pub snooze_duration: Option<i64>,
}

/// Trigger action event data.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TriggerActionData {
    /// Trigger that received the action
    #[validate(length(min = 1, max = 128))]
    pub trigger_id: String,
    /// Type of action taken (e.g., "click", "submit", "navigate")
    #[validate(length(min = 1, max = 64))]
    pub action_type: String,
    /// Custom developer data associated with the action (max 16KB)
    #[validate(custom(function = "validate_properties_size"))]
    pub data: Option<serde_json::Value>,
}

/// Trigger error type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerErrorType {
    /// Error evaluating trigger condition
    ConditionEval,
    /// Error in fire callback
    FireCallback,
    /// Error accessing storage
    Storage,
}

/// Trigger error event data.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TriggerErrorData {
    /// Trigger that encountered the error
    #[validate(length(min = 1, max = 128))]
    pub trigger_id: String,
    /// Type of error
    pub error_type: TriggerErrorType,
    /// Error message
    #[validate(length(max = 1000))]
    pub message: String,
    /// Optional stack trace
    #[validate(length(max = 4000))]
    pub stack: Option<String>,
}

/// Event payload variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum EventPayload {
    Pageview(PageviewData),
    Click(ClickData),
    Scroll(ScrollData),
    Performance(PerformanceData),
    Custom(CustomData),
}

impl EventPayload {
    /// Returns the event type as a string.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Pageview(_) => "pageview",
            Self::Click(_) => "click",
            Self::Scroll(_) => "scroll",
            Self::Performance(_) => "performance",
            Self::Custom(_) => "custom",
        }
    }

    /// Returns the Redpanda topic for this event type.
    pub fn topic(&self) -> &'static str {
        match self {
            Self::Pageview(_) => "events_pageview",
            Self::Click(_) => "events_click",
            Self::Scroll(_) => "events_scroll",
            Self::Performance(_) => "events_performance",
            Self::Custom(_) => "events_custom",
        }
    }
}

/// A single analytics event.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Event {
    /// Unique event ID
    pub id: Uuid,
    /// Tenant/project ID
    pub tenant_id: Uuid,
    /// Session ID for ordering guarantees
    pub session_id: Uuid,
    /// Optional user ID
    #[validate(length(max = 128))]
    pub user_id: Option<String>,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Server receive timestamp
    #[serde(default = "Utc::now")]
    pub received_at: DateTime<Utc>,
    /// Event payload
    #[serde(flatten)]
    pub payload: EventPayload,
    /// Client metadata
    pub metadata: Option<EventMetadata>,
}

/// Client-side metadata attached to events.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct EventMetadata {
    /// User agent string
    #[validate(length(max = 512))]
    pub user_agent: Option<String>,
    /// Client IP (set by server)
    #[validate(length(max = 45))]
    pub ip: Option<String>,
    /// Screen dimensions
    pub screen_width: Option<u32>,
    pub screen_height: Option<u32>,
    /// Viewport dimensions
    pub viewport_width: Option<u32>,
    pub viewport_height: Option<u32>,
    /// Device pixel ratio
    pub device_pixel_ratio: Option<f64>,
    /// Timezone
    #[validate(length(max = 64))]
    pub timezone: Option<String>,
    /// Language
    #[validate(length(max = 16))]
    pub language: Option<String>,
}

/// Batch of events from SDK.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct EventBatch {
    /// Events in this batch
    #[validate(length(min = 1, max = 1000))]
    pub events: Vec<Event>,
}

impl Event {
    /// Creates a new event with generated ID and timestamps.
    pub fn new(tenant_id: Uuid, session_id: Uuid, payload: EventPayload) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            session_id,
            user_id: None,
            timestamp: now,
            received_at: now,
            payload,
            metadata: None,
        }
    }

    /// Returns the partition key for Redpanda (session-based ordering).
    pub fn partition_key(&self) -> String {
        self.session_id.to_string()
    }
}
