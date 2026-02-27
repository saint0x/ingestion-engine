//! Mock implementations for testing.

use async_trait::async_trait;
use engine_core::{ClickHouseEvent, Result};
use parking_lot::Mutex;
use redpanda::{EventProducer, SendResult};
use std::sync::Arc;

/// Mock producer that captures events in memory.
///
/// This implements the same `EventProducer` trait as the real `Producer`,
/// allowing tests to verify the exact events that would be sent to Kafka
/// without actually connecting to a Kafka broker.
#[derive(Clone)]
pub struct MockProducer {
    /// All events sent through this producer.
    events: Arc<Mutex<Vec<ClickHouseEvent>>>,
    /// Simulate failures if set.
    should_fail: Arc<Mutex<bool>>,
}

impl MockProducer {
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            should_fail: Arc::new(Mutex::new(false)),
        }
    }

    /// Get all captured events.
    pub fn captured_events(&self) -> Vec<ClickHouseEvent> {
        self.events.lock().clone()
    }

    /// Get the count of captured events.
    pub fn event_count(&self) -> usize {
        self.events.lock().len()
    }

    /// Clear captured events.
    pub fn clear(&self) {
        self.events.lock().clear();
    }

    /// Set failure mode for testing error handling.
    pub fn set_should_fail(&self, fail: bool) {
        *self.should_fail.lock() = fail;
    }
}

impl Default for MockProducer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventProducer for MockProducer {
    async fn send_clickhouse_events(&self, events: Vec<ClickHouseEvent>) -> Result<SendResult> {
        if *self.should_fail.lock() {
            return Err(engine_core::Error::internal("Mock producer failure"));
        }

        let count = events.len();
        self.events.lock().extend(events);

        Ok(SendResult {
            events_sent: count,
            errors: vec![],
        })
    }

    fn is_healthy(&self) -> bool {
        !*self.should_fail.lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_event(id: &str) -> ClickHouseEvent {
        ClickHouseEvent {
            event_id: id.into(),
            project_id: "proj-test".into(),
            session_id: "sess-123".into(),
            user_id: None,
            event_type: "pageview".into(),
            custom_name: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
            url: "https://example.com".into(),
            path: "/".into(),
            referrer: "".into(),
            user_agent: "Mozilla/5.0".into(),
            device_type: "desktop".into(),
            browser: "Chrome".into(),
            browser_version: "120.0".into(),
            os: "macOS".into(),
            country: "US".into(),
            region: None,
            city: None,
            data: "{}".into(),
        }
    }

    #[tokio::test]
    async fn test_mock_producer_captures_events() {
        let mock = MockProducer::new();

        let events = vec![test_event("e1")];

        let result = mock.send_clickhouse_events(events).await.unwrap();
        assert_eq!(result.events_sent, 1);
        assert_eq!(mock.event_count(), 1);

        let captured = mock.captured_events();
        assert_eq!(captured[0].event_id, "e1");
    }

    #[tokio::test]
    async fn test_mock_producer_failure_mode() {
        let mock = MockProducer::new();
        mock.set_should_fail(true);

        let events = vec![];
        let result = mock.send_clickhouse_events(events).await;
        assert!(result.is_err());
        assert!(!mock.is_healthy());
    }
}
