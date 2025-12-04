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
    Pageview,
    Pageleave,
    Click,
    Scroll,
    MouseMove,
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
}

impl EventType {
    /// Returns the string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pageview => "pageview",
            Self::Pageleave => "pageleave",
            Self::Click => "click",
            Self::Scroll => "scroll",
            Self::MouseMove => "mouse_move",
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
        }
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
        let value: Value =
            serde_json::from_slice(bytes).map_err(|e| Error::validation(format!("invalid JSON: {}", e)))?;

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

        Ok(Self {
            event_id: event.id,
            project_id: project_id.to_string(),
            session_id: event.session_id,
            user_id: event.user_id,
            event_type: event.event_type.as_str().to_string(),
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
        return Err(Error::validation("timestamp cannot be more than 5s in the future"));
    }
    if event.timestamp < now - max_past {
        return Err(Error::validation("timestamp cannot be more than 24h in the past"));
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
    }
}
