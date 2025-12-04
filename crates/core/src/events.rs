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
