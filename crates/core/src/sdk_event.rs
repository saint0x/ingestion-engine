//! SDK event types and transformation to ClickHouse format.
//!
//! This module handles:
//! - Parsing SDK events (camelCase, Unix ms timestamps)
//! - Validating required fields
//! - Transforming to ClickHouse format (snake_case, DateTime)
//! - Supporting 3 payload formats (array, object with events, single)

use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use validator::Validate;

use crate::error::{Error, Result};
use crate::limits::MAX_CUSTOM_PROPERTIES_BYTES;

/// All supported event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    // Core analytics events
    Pageview,
    Pageleave,
    Click,
    Scroll,
    MouseMove,
    Keydown,
    Keypress,
    Keyup,
    FormFocus,
    FormBlur,
    FormSubmit,
    FormAbandon,
    Error,
    VisibilityChange,
    ResourceLoad,
    SessionStart,
    SessionEnd,
    Performance,
    Custom,

    // Overwatch Triggers v1.0 - Context-based notification system
    /// Exit intent detected (mouse moving toward browser chrome)
    ExitIntent,
    /// User became idle (no activity for threshold period)
    IdleStart,
    /// User resumed activity after being idle
    IdleEnd,
    /// Periodic engagement score snapshot
    EngagementSnapshot,
    /// A trigger was registered by the SDK
    TriggerRegistered,
    /// A trigger condition was met and fired
    TriggerFired,
    /// A trigger was dismissed by user or system
    TriggerDismissed,
    /// User took action on a triggered notification
    TriggerAction,
    /// Error occurred during trigger evaluation or firing
    TriggerError,
}

impl EventType {
    /// Returns the string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            // Core analytics events
            Self::Pageview => "pageview",
            Self::Pageleave => "pageleave",
            Self::Click => "click",
            Self::Scroll => "scroll",
            Self::MouseMove => "mouse_move",
            Self::Keydown => "keydown",
            Self::Keypress => "keypress",
            Self::Keyup => "keyup",
            Self::FormFocus => "form_focus",
            Self::FormBlur => "form_blur",
            Self::FormSubmit => "form_submit",
            Self::FormAbandon => "form_abandon",
            Self::Error => "error",
            Self::VisibilityChange => "visibility_change",
            Self::ResourceLoad => "resource_load",
            Self::SessionStart => "session_start",
            Self::SessionEnd => "session_end",
            Self::Performance => "performance",
            Self::Custom => "custom",

            // Overwatch Triggers v1.0
            Self::ExitIntent => "exit_intent",
            Self::IdleStart => "idle_start",
            Self::IdleEnd => "idle_end",
            Self::EngagementSnapshot => "engagement_snapshot",
            Self::TriggerRegistered => "trigger_registered",
            Self::TriggerFired => "trigger_fired",
            Self::TriggerDismissed => "trigger_dismissed",
            Self::TriggerAction => "trigger_action",
            Self::TriggerError => "trigger_error",
        }
    }

    /// Returns true if this is an Overwatch Triggers event type.
    pub fn is_trigger_event(&self) -> bool {
        matches!(
            self,
            Self::ExitIntent
                | Self::IdleStart
                | Self::IdleEnd
                | Self::EngagementSnapshot
                | Self::TriggerRegistered
                | Self::TriggerFired
                | Self::TriggerDismissed
                | Self::TriggerAction
                | Self::TriggerError
        )
    }

    /// Returns true if this is a high-volume event type that may need sampling.
    pub fn is_high_volume(&self) -> bool {
        matches!(self, Self::MouseMove | Self::EngagementSnapshot)
    }
}

/// Device information from SDK.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceInfo {
    pub device: Option<DeviceDetails>,
    pub browser: Option<BrowserDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceDetails {
    #[serde(rename = "type")]
    pub device_type: Option<String>,
    pub os: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BrowserDetails {
    pub name: Option<String>,
    pub version: Option<String>,
}

/// Location information from SDK.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocationInfo {
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
}

/// SDK event as received from client (camelCase).
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct SDKEvent {
    /// Event ID (UUID format)
    pub id: String,

    /// Event type
    #[serde(rename = "type")]
    pub event_type: EventType,

    /// Unix timestamp in milliseconds
    pub timestamp: i64,

    /// Session ID
    pub session_id: String,

    /// Full page URL
    #[validate(length(max = 2048))]
    pub url: String,

    /// Browser user agent
    #[validate(length(max = 512))]
    pub user_agent: String,

    /// Optional user ID
    #[validate(length(max = 128))]
    pub user_id: Option<String>,

    /// URL path (extracted from url if missing)
    #[validate(length(max = 2000))]
    pub path: Option<String>,

    /// Referrer URL
    #[validate(length(max = 2048))]
    pub referrer: Option<String>,

    /// Device information
    pub device_info: Option<DeviceInfo>,

    /// Location information
    pub location: Option<LocationInfo>,

    /// Extra fields captured as JSON
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// SDK metadata sent with batch.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SDKMetadata {
    pub sdk_version: Option<String>,
    pub timestamp: Option<i64>,
    pub queue_size: Option<u32>,
}

/// Parsed SDK payload (supports 3 formats).
#[derive(Debug, Clone)]
pub struct SDKPayload {
    pub events: Vec<SDKEvent>,
    pub metadata: Option<SDKMetadata>,
}

impl SDKPayload {
    /// Parse SDK payload from JSON bytes.
    /// Supports:
    /// 1. Array: `[event, event, ...]`
    /// 2. Object with events: `{ "events": [...], "metadata": {...} }`
    /// 3. Single event: `{ "id": "...", "type": "...", ... }`
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let value: Value = serde_json::from_slice(bytes)
            .map_err(|e| Error::validation(format!("invalid JSON: {}", e)))?;

        match &value {
            // Format 1: Array of events
            Value::Array(_) => {
                let events: Vec<SDKEvent> = serde_json::from_value(value)
                    .map_err(|e| Error::validation(format!("invalid event array: {}", e)))?;
                Ok(Self {
                    events,
                    metadata: None,
                })
            }

            // Format 2 or 3: Object
            Value::Object(obj) => {
                if obj.contains_key("events") {
                    // Format 2: Object with events array
                    #[derive(Deserialize)]
                    struct Wrapper {
                        events: Vec<SDKEvent>,
                        metadata: Option<SDKMetadata>,
                    }
                    let wrapper: Wrapper = serde_json::from_value(value)
                        .map_err(|e| Error::validation(format!("invalid batch object: {}", e)))?;
                    Ok(Self {
                        events: wrapper.events,
                        metadata: wrapper.metadata,
                    })
                } else if obj.contains_key("id") && obj.contains_key("type") {
                    // Format 3: Single event
                    let event: SDKEvent = serde_json::from_value(value)
                        .map_err(|e| Error::validation(format!("invalid single event: {}", e)))?;
                    Ok(Self {
                        events: vec![event],
                        metadata: None,
                    })
                } else {
                    Err(Error::validation(
                        "object must have 'events' array or be a single event with 'id' and 'type'",
                    ))
                }
            }

            _ => Err(Error::validation(
                "request body must be an array of events or an object",
            )),
        }
    }
}

/// ClickHouse event row (snake_case).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickHouseEvent {
    pub event_id: String,
    pub project_id: String,
    pub session_id: String,
    pub user_id: Option<String>,
    #[serde(rename = "type")]
    pub event_type: String,
    pub custom_name: Option<String>,
    pub timestamp: i64, // DateTime64(3) as milliseconds since epoch
    pub url: String,
    pub path: String,
    pub referrer: String,
    pub user_agent: String,
    pub device_type: String,
    pub browser: String,
    pub browser_version: String,
    pub os: String,
    pub country: String,
    pub region: Option<String>,
    pub city: Option<String>,
    pub data: String, // JSON string of extra fields
}

impl ClickHouseEvent {
    /// Transform SDK event to ClickHouse format.
    pub fn from_sdk(event: SDKEvent, project_id: &str) -> Result<Self> {
        // Convert Unix ms to DateTime
        let timestamp = Utc
            .timestamp_millis_opt(event.timestamp)
            .single()
            .ok_or_else(|| Error::validation("invalid timestamp"))?;

        // Extract path from URL if not provided
        let path = event.path.unwrap_or_else(|| extract_path(&event.url));

        // Extract device info
        let (device_type, os) = event
            .device_info
            .as_ref()
            .and_then(|d| d.device.as_ref())
            .map(|d| {
                (
                    d.device_type.clone().unwrap_or_else(|| "unknown".into()),
                    d.os.clone().unwrap_or_else(|| "unknown".into()),
                )
            })
            .unwrap_or_else(|| ("unknown".into(), "unknown".into()));

        // Extract browser info
        let (browser, browser_version) = event
            .device_info
            .as_ref()
            .and_then(|d| d.browser.as_ref())
            .map(|b| {
                (
                    b.name.clone().unwrap_or_else(|| "unknown".into()),
                    b.version.clone().unwrap_or_else(|| "unknown".into()),
                )
            })
            .unwrap_or_else(|| ("unknown".into(), "unknown".into()));

        // Extract location info
        let (country, region, city) = event
            .location
            .as_ref()
            .map(|l| {
                (
                    l.country.clone().unwrap_or_else(|| "unknown".into()),
                    l.region.clone(),
                    l.city.clone(),
                )
            })
            .unwrap_or_else(|| ("unknown".into(), None, None));

        // Serialize extra fields to JSON, excluding known fields
        let data = if event.extra.is_empty() {
            "{}".to_string()
        } else {
            serde_json::to_string(&event.extra).unwrap_or_else(|_| "{}".into())
        };

        // Validate data size
        if data.len() > MAX_CUSTOM_PROPERTIES_BYTES {
            return Err(Error::validation(format!(
                "extra data {}KB exceeds {}KB limit",
                data.len() / 1024,
                MAX_CUSTOM_PROPERTIES_BYTES / 1024
            )));
        }

        let custom_name = if matches!(event.event_type, EventType::Custom) {
            event
                .extra
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        } else {
            None
        };

        Ok(Self {
            event_id: event.id,
            project_id: project_id.to_string(),
            session_id: event.session_id,
            user_id: event.user_id,
            event_type: event.event_type.as_str().to_string(),
            custom_name,
            timestamp: timestamp.timestamp_millis(),
            url: event.url,
            path,
            referrer: event.referrer.unwrap_or_default(),
            user_agent: event.user_agent,
            device_type,
            browser,
            browser_version,
            os,
            country,
            region,
            city,
            data,
        })
    }
}

/// Extract path from URL.
fn extract_path(url: &str) -> String {
    url::Url::parse(url)
        .map(|u| u.path().to_string())
        .unwrap_or_else(|_| "/".to_string())
}

/// Validate an SDK event.
pub fn validate_sdk_event(event: &SDKEvent) -> Result<()> {
    // Run validator derive validations
    event
        .validate()
        .map_err(|e| Error::validation(format!("{}", e)))?;

    // Validate required fields are non-empty
    if event.id.is_empty() {
        return Err(Error::validation("id is required"));
    }
    if event.session_id.is_empty() {
        return Err(Error::validation("sessionId is required"));
    }
    if event.url.is_empty() {
        return Err(Error::validation("url is required"));
    }
    if event.user_agent.is_empty() {
        return Err(Error::validation("userAgent is required"));
    }

    // Validate timestamp is reasonable (within ±24h)
    let now = Utc::now().timestamp_millis();
    let max_future = 5 * 1000; // 5 seconds
    let max_past = 24 * 60 * 60 * 1000; // 24 hours

    if event.timestamp > now + max_future {
        return Err(Error::validation(
            "timestamp cannot be more than 5s in the future",
        ));
    }
    if event.timestamp < now - max_past {
        return Err(Error::validation(
            "timestamp cannot be more than 24h in the past",
        ));
    }

    // Validate event-specific payload shape.
    validate_event_data(event)?;

    Ok(())
}

/// Validate event-specific payload shape.
fn validate_event_data(event: &SDKEvent) -> Result<()> {
    use crate::events::*;
    use validator::Validate;

    // Convert extra fields to JSON value for parsing
    let data = serde_json::to_value(&event.extra).unwrap_or(Value::Null);

    match event.event_type {
        EventType::ExitIntent => {
            let exit_data = serde_json::from_value::<ExitIntentData>(data)
                .map_err(|e| Error::validation(format!("exit_intent data: {}", e)))?;
            exit_data
                .validate()
                .map_err(|e| Error::validation(format!("exit_intent data: {}", e)))?;
        }
        EventType::IdleStart => {
            let idle_data = serde_json::from_value::<IdleStartData>(data)
                .map_err(|e| Error::validation(format!("idle_start data: {}", e)))?;
            idle_data
                .validate()
                .map_err(|e| Error::validation(format!("idle_start data: {}", e)))?;
        }
        EventType::IdleEnd => {
            let idle_data = serde_json::from_value::<IdleEndData>(data)
                .map_err(|e| Error::validation(format!("idle_end data: {}", e)))?;
            idle_data
                .validate()
                .map_err(|e| Error::validation(format!("idle_end data: {}", e)))?;
        }
        EventType::EngagementSnapshot => {
            let engagement_data = serde_json::from_value::<EngagementSnapshotData>(data)
                .map_err(|e| Error::validation(format!("engagement_snapshot data: {}", e)))?;
            engagement_data
                .validate()
                .map_err(|e| Error::validation(format!("engagement_snapshot data: {}", e)))?;
            // Additional range validation for score
            if engagement_data.score < 0.0 || engagement_data.score > 100.0 {
                return Err(Error::validation("engagement_snapshot score must be 0-100"));
            }
        }
        EventType::TriggerRegistered => {
            let trigger_data = serde_json::from_value::<TriggerRegisteredData>(data)
                .map_err(|e| Error::validation(format!("trigger_registered data: {}", e)))?;
            trigger_data
                .validate()
                .map_err(|e| Error::validation(format!("trigger_registered data: {}", e)))?;
        }
        EventType::TriggerFired => {
            let trigger_data = serde_json::from_value::<TriggerFiredData>(data)
                .map_err(|e| Error::validation(format!("trigger_fired data: {}", e)))?;
            trigger_data
                .validate()
                .map_err(|e| Error::validation(format!("trigger_fired data: {}", e)))?;
        }
        EventType::TriggerDismissed => {
            let trigger_data = serde_json::from_value::<TriggerDismissedData>(data)
                .map_err(|e| Error::validation(format!("trigger_dismissed data: {}", e)))?;
            trigger_data
                .validate()
                .map_err(|e| Error::validation(format!("trigger_dismissed data: {}", e)))?;
        }
        EventType::TriggerAction => {
            let trigger_data = serde_json::from_value::<TriggerActionData>(data)
                .map_err(|e| Error::validation(format!("trigger_action data: {}", e)))?;
            trigger_data
                .validate()
                .map_err(|e| Error::validation(format!("trigger_action data: {}", e)))?;
        }
        EventType::TriggerError => {
            let trigger_data = serde_json::from_value::<TriggerErrorData>(data)
                .map_err(|e| Error::validation(format!("trigger_error data: {}", e)))?;
            trigger_data
                .validate()
                .map_err(|e| Error::validation(format!("trigger_error data: {}", e)))?;
        }
        // MouseMove already exists, but validate enhanced fields if present
        EventType::MouseMove => {
            let mouse_data = serde_json::from_value::<MouseMoveData>(data)
                .map_err(|e| Error::validation(format!("mouse_move data: {}", e)))?;
            mouse_data
                .validate()
                .map_err(|e| Error::validation(format!("mouse_move data: {}", e)))?;
        }
        EventType::Custom => {
            let custom_data: SDKCustomData = serde_json::from_value(data)
                .map_err(|e| Error::validation(format!("custom data: {}", e)))?;
            custom_data
                .validate()
                .map_err(|e| Error::validation(format!("custom data: {}", e)))?;
            validate_custom_properties_size(&custom_data.properties)?;
        }
        _ => {}
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
struct SDKCustomData {
    #[validate(length(min = 1, max = 100))]
    name: String,
    #[serde(default)]
    properties: Value,
}

fn validate_custom_properties_size(properties: &Value) -> Result<()> {
    if !properties.is_object() {
        return Err(Error::validation("custom properties must be a JSON object"));
    }

    let size = serde_json::to_vec(properties)
        .map(|v| v.len())
        .map_err(|e| Error::validation(format!("custom properties invalid JSON: {}", e)))?;

    if size > MAX_CUSTOM_PROPERTIES_BYTES {
        return Err(Error::validation(format!(
            "custom properties {}KB exceeds {}KB limit",
            size / 1024,
            MAX_CUSTOM_PROPERTIES_BYTES / 1024
        )));
    }

    Ok(())
}

/// Validate and transform a batch of SDK events.
pub fn transform_batch(
    events: Vec<SDKEvent>,
    project_id: &str,
) -> Result<(Vec<ClickHouseEvent>, Vec<Error>)> {
    let mut transformed = Vec::with_capacity(events.len());
    let mut errors = Vec::new();

    for (i, event) in events.into_iter().enumerate() {
        if let Err(e) = validate_sdk_event(&event) {
            errors.push(Error::validation(format!("event[{}]: {}", i, e)));
            continue;
        }

        match ClickHouseEvent::from_sdk(event, project_id) {
            Ok(ch_event) => transformed.push(ch_event),
            Err(e) => errors.push(Error::validation(format!("event[{}]: {}", i, e))),
        }
    }

    Ok((transformed, errors))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_sdk_event() -> SDKEvent {
        SDKEvent {
            id: "550e8400-e29b-41d4-a716-446655440000".into(),
            event_type: EventType::Pageview,
            timestamp: Utc::now().timestamp_millis(),
            session_id: "11111111-1111-1111-1111-111111111111".into(),
            url: "https://example.com/page".into(),
            user_agent: "Mozilla/5.0".into(),
            user_id: None,
            path: None,
            referrer: None,
            device_info: None,
            location: None,
            extra: HashMap::new(),
        }
    }

    #[test]
    fn test_parse_array_format() {
        let json = r#"[{"id":"1","type":"pageview","timestamp":1234567890000,"sessionId":"s1","url":"https://example.com","userAgent":"Mozilla"}]"#;
        let payload = SDKPayload::parse(json.as_bytes()).unwrap();
        assert_eq!(payload.events.len(), 1);
        assert!(payload.metadata.is_none());
    }

    #[test]
    fn test_parse_object_format() {
        let json = r#"{"events":[{"id":"1","type":"pageview","timestamp":1234567890000,"sessionId":"s1","url":"https://example.com","userAgent":"Mozilla"}],"metadata":{"sdkVersion":"1.0"}}"#;
        let payload = SDKPayload::parse(json.as_bytes()).unwrap();
        assert_eq!(payload.events.len(), 1);
        assert!(payload.metadata.is_some());
    }

    #[test]
    fn test_parse_single_event_format() {
        let json = r#"{"id":"1","type":"pageview","timestamp":1234567890000,"sessionId":"s1","url":"https://example.com","userAgent":"Mozilla"}"#;
        let payload = SDKPayload::parse(json.as_bytes()).unwrap();
        assert_eq!(payload.events.len(), 1);
    }

    #[test]
    fn test_transform_to_clickhouse() {
        let event = valid_sdk_event();
        let ch_event = ClickHouseEvent::from_sdk(event, "project-123").unwrap();
        assert_eq!(ch_event.project_id, "project-123");
        assert_eq!(ch_event.event_type, "pageview");
        assert_eq!(ch_event.device_type, "unknown");
    }

    #[test]
    fn test_extract_path() {
        assert_eq!(extract_path("https://example.com/foo/bar"), "/foo/bar");
        assert_eq!(extract_path("https://example.com"), "/");
        assert_eq!(extract_path("invalid"), "/");
    }

    #[test]
    fn test_validate_required_fields() {
        let mut event = valid_sdk_event();
        event.id = "".into();
        assert!(validate_sdk_event(&event).is_err());
    }

    #[test]
    fn test_event_types() {
        assert_eq!(EventType::Pageview.as_str(), "pageview");
        assert_eq!(EventType::FormSubmit.as_str(), "form_submit");
        assert_eq!(EventType::VisibilityChange.as_str(), "visibility_change");
        assert_eq!(EventType::Keydown.as_str(), "keydown");
    }

    // ==========================================================================
    // Overwatch Triggers v1.0 Tests
    // ==========================================================================

    #[test]
    fn test_trigger_event_types() {
        // Test new trigger event type string representations
        assert_eq!(EventType::ExitIntent.as_str(), "exit_intent");
        assert_eq!(EventType::IdleStart.as_str(), "idle_start");
        assert_eq!(EventType::IdleEnd.as_str(), "idle_end");
        assert_eq!(
            EventType::EngagementSnapshot.as_str(),
            "engagement_snapshot"
        );
        assert_eq!(EventType::TriggerRegistered.as_str(), "trigger_registered");
        assert_eq!(EventType::TriggerFired.as_str(), "trigger_fired");
        assert_eq!(EventType::TriggerDismissed.as_str(), "trigger_dismissed");
        assert_eq!(EventType::TriggerAction.as_str(), "trigger_action");
        assert_eq!(EventType::TriggerError.as_str(), "trigger_error");
    }

    #[test]
    fn test_is_trigger_event() {
        // Trigger events
        assert!(EventType::ExitIntent.is_trigger_event());
        assert!(EventType::IdleStart.is_trigger_event());
        assert!(EventType::IdleEnd.is_trigger_event());
        assert!(EventType::EngagementSnapshot.is_trigger_event());
        assert!(EventType::TriggerRegistered.is_trigger_event());
        assert!(EventType::TriggerFired.is_trigger_event());
        assert!(EventType::TriggerDismissed.is_trigger_event());
        assert!(EventType::TriggerAction.is_trigger_event());
        assert!(EventType::TriggerError.is_trigger_event());

        // Non-trigger events
        assert!(!EventType::Pageview.is_trigger_event());
        assert!(!EventType::Click.is_trigger_event());
        assert!(!EventType::MouseMove.is_trigger_event());
        assert!(!EventType::Custom.is_trigger_event());
    }

    #[test]
    fn test_is_high_volume() {
        assert!(EventType::MouseMove.is_high_volume());
        assert!(EventType::EngagementSnapshot.is_high_volume());
        assert!(!EventType::Pageview.is_high_volume());
        assert!(!EventType::TriggerFired.is_high_volume());
    }

    #[test]
    fn test_parse_exit_intent_event() {
        let json = r#"{"id":"1","type":"exit_intent","timestamp":1234567890000,"sessionId":"s1","url":"https://example.com","userAgent":"Mozilla","position":{"x":100,"y":0},"velocity":500.0,"timeOnPage":30000,"scrollDepth":75.5}"#;
        let payload = SDKPayload::parse(json.as_bytes()).unwrap();
        assert_eq!(payload.events.len(), 1);
        assert_eq!(payload.events[0].event_type, EventType::ExitIntent);
    }

    #[test]
    fn test_parse_idle_start_event() {
        let json = r#"{"id":"1","type":"idle_start","timestamp":1234567890000,"sessionId":"s1","url":"https://example.com","userAgent":"Mozilla","lastActivityType":"mouse","timeOnPage":60000}"#;
        let payload = SDKPayload::parse(json.as_bytes()).unwrap();
        assert_eq!(payload.events.len(), 1);
        assert_eq!(payload.events[0].event_type, EventType::IdleStart);
    }

    #[test]
    fn test_parse_engagement_snapshot_event() {
        let json = r#"{"id":"1","type":"engagement_snapshot","timestamp":1234567890000,"sessionId":"s1","url":"https://example.com","userAgent":"Mozilla","score":75.5,"factors":{"timeOnPage":30000,"scrollDepth":50.0,"clickCount":5,"formInteraction":true,"mouseActivity":1500.0,"focusTime":25000}}"#;
        let payload = SDKPayload::parse(json.as_bytes()).unwrap();
        assert_eq!(payload.events.len(), 1);
        assert_eq!(payload.events[0].event_type, EventType::EngagementSnapshot);
    }

    #[test]
    fn test_parse_keydown_event() {
        let json = r#"{"id":"1","type":"keydown","timestamp":1234567890000,"sessionId":"s1","url":"https://example.com","userAgent":"Mozilla","key":"Enter","element":"input","formField":"email"}"#;
        let payload = SDKPayload::parse(json.as_bytes()).unwrap();
        assert_eq!(payload.events.len(), 1);
        assert_eq!(payload.events[0].event_type, EventType::Keydown);
    }

    #[test]
    fn test_custom_event_requires_name_and_properties_object() {
        let mut event = valid_sdk_event();
        event.event_type = EventType::Custom;
        event.extra = HashMap::new();
        assert!(validate_sdk_event(&event).is_err());

        let mut event_with_name = valid_sdk_event();
        event_with_name.event_type = EventType::Custom;
        event_with_name
            .extra
            .insert("name".into(), Value::String("purchase".into()));
        event_with_name
            .extra
            .insert("properties".into(), Value::String("invalid".into()));
        assert!(validate_sdk_event(&event_with_name).is_err());

        let mut valid_custom = valid_sdk_event();
        valid_custom.event_type = EventType::Custom;
        valid_custom
            .extra
            .insert("name".into(), Value::String("purchase".into()));
        valid_custom.extra.insert(
            "properties".into(),
            serde_json::json!({ "amount": 99.99, "currency": "USD" }),
        );
        assert!(validate_sdk_event(&valid_custom).is_ok());
    }

    #[test]
    fn test_parse_trigger_fired_event() {
        let json = r#"{"id":"1","type":"trigger_fired","timestamp":1234567890000,"sessionId":"s1","url":"https://example.com","userAgent":"Mozilla","triggerId":"promo-banner-1","condition":"scroll_depth>50","priority":100,"context":{"timeOnPage":30000,"scrollDepth":55.0,"engagementScore":70.0,"sessionDuration":120000,"pageCount":3}}"#;
        let payload = SDKPayload::parse(json.as_bytes()).unwrap();
        assert_eq!(payload.events.len(), 1);
        assert_eq!(payload.events[0].event_type, EventType::TriggerFired);
    }

    #[test]
    fn test_parse_trigger_action_event() {
        let json = r#"{"id":"1","type":"trigger_action","timestamp":1234567890000,"sessionId":"s1","url":"https://example.com","userAgent":"Mozilla","triggerId":"promo-banner-1","actionType":"click","data":{"buttonId":"cta-signup"}}"#;
        let payload = SDKPayload::parse(json.as_bytes()).unwrap();
        assert_eq!(payload.events.len(), 1);
        assert_eq!(payload.events[0].event_type, EventType::TriggerAction);
    }

    #[test]
    fn test_transform_trigger_event_to_clickhouse() {
        let mut event = valid_sdk_event();
        event.event_type = EventType::TriggerFired;
        event
            .extra
            .insert("triggerId".into(), Value::String("test-trigger".into()));
        event
            .extra
            .insert("condition".into(), Value::String("scroll>50".into()));
        event
            .extra
            .insert("priority".into(), Value::Number(100.into()));

        let ch_event = ClickHouseEvent::from_sdk(event, "project-123").unwrap();
        assert_eq!(ch_event.event_type, "trigger_fired");
        assert!(ch_event.data.contains("triggerId"));
        assert!(ch_event.data.contains("test-trigger"));
    }

    #[test]
    fn test_transform_custom_event_includes_custom_name() {
        let mut event = valid_sdk_event();
        event.event_type = EventType::Custom;
        event
            .extra
            .insert("name".into(), Value::String("purchase".into()));
        event
            .extra
            .insert("properties".into(), serde_json::json!({"value": 42}));

        let ch_event = ClickHouseEvent::from_sdk(event, "project-123").unwrap();
        assert_eq!(ch_event.event_type, "custom");
        assert_eq!(ch_event.custom_name.as_deref(), Some("purchase"));
    }

    #[test]
    fn test_serde_roundtrip_all_trigger_types() {
        let event_types = vec![
            EventType::ExitIntent,
            EventType::IdleStart,
            EventType::IdleEnd,
            EventType::EngagementSnapshot,
            EventType::TriggerRegistered,
            EventType::TriggerFired,
            EventType::TriggerDismissed,
            EventType::TriggerAction,
            EventType::TriggerError,
        ];

        for event_type in event_types {
            let json = serde_json::to_string(&event_type).unwrap();
            let parsed: EventType = serde_json::from_str(&json).unwrap();
            assert_eq!(event_type, parsed, "Failed roundtrip for {:?}", event_type);
        }
    }
}
