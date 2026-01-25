//! Test fixtures and event generators.

use chrono::Utc;
use uuid::Uuid;

/// Generate a valid SDK event JSON with unique IDs.
pub fn sdk_event(event_type: &str) -> serde_json::Value {
    serde_json::json!({
        "id": Uuid::new_v4().to_string(),
        "type": event_type,
        "timestamp": Utc::now().timestamp_millis(),
        "sessionId": Uuid::new_v4().to_string(),
        "url": "https://example.com/test",
        "userAgent": "Mozilla/5.0 (Test)"
    })
}

/// Generate a valid SDK event with a specific session ID.
pub fn sdk_event_with_session(event_type: &str, session_id: &str) -> serde_json::Value {
    serde_json::json!({
        "id": Uuid::new_v4().to_string(),
        "type": event_type,
        "timestamp": Utc::now().timestamp_millis(),
        "sessionId": session_id,
        "url": "https://example.com/test",
        "userAgent": "Mozilla/5.0 (Test)"
    })
}

/// Generate N valid SDK events.
pub fn sdk_events(n: usize) -> Vec<serde_json::Value> {
    (0..n).map(|_| sdk_event("pageview")).collect()
}

/// Generate N valid SDK events of a specific type.
pub fn sdk_events_of_type(n: usize, event_type: &str) -> Vec<serde_json::Value> {
    (0..n).map(|_| sdk_event(event_type)).collect()
}

/// Generate a valid API key for testing.
pub fn test_api_key() -> String {
    "owk_test_ABC123xyz789DEF456ghi012JKL345mn".to_string()
}

/// Generate a live API key for testing.
pub fn live_api_key() -> String {
    "owk_live_ABC123xyz789DEF456ghi012JKL345mn".to_string()
}

/// Generate array format payload.
pub fn array_payload(events: Vec<serde_json::Value>) -> String {
    serde_json::to_string(&events).unwrap()
}

/// Generate object format payload with metadata.
pub fn object_payload(events: Vec<serde_json::Value>) -> String {
    serde_json::json!({
        "events": events,
        "metadata": { "sdkVersion": "1.0.0" }
    })
    .to_string()
}

/// Generate single event payload.
pub fn single_payload(event: serde_json::Value) -> String {
    event.to_string()
}

/// Generate an oversized event (> 64KB).
pub fn oversized_event() -> serde_json::Value {
    let mut event = sdk_event("custom");
    // 70KB of data exceeds the 64KB limit
    event["hugeField"] = serde_json::Value::String("x".repeat(70_000));
    event
}

/// Generate a batch that exceeds the event limit.
pub fn oversized_batch() -> Vec<serde_json::Value> {
    sdk_events(1001) // Exceeds 1000 limit
}

/// Calculate expected project_id from API key (matches AuthClient mock behavior).
/// The mock implementation uses a hash of the API key to generate a deterministic project_id.
pub fn expected_project_id(api_key: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    api_key.hash(&mut hasher);
    let hash = hasher.finish();
    // Must match generate_mock_project_id in api/state.rs
    format!("proj-{:016x}", hash)
}

/// Calculate expected workspace_id from API key (matches AuthClient mock behavior).
/// The mock implementation uses a hash of the API key to generate a deterministic workspace_id.
pub fn expected_workspace_id(api_key: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    // Use a different seed to get a different hash (matches generate_mock_workspace_id)
    "workspace".hash(&mut hasher);
    api_key.hash(&mut hasher);
    let hash = hasher.finish();
    // Must match generate_mock_workspace_id in api/state.rs
    format!("ws-{:016x}", hash)
}
