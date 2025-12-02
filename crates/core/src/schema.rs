//! Schema validation for events.

use chrono::Duration;

use crate::error::{Error, Result};
use crate::events::{Event, EventBatch, EventPayload};
use crate::limits::{
    MAX_BATCH_SIZE_BYTES, MAX_EVENT_AGE_HOURS, MAX_EVENT_SIZE_BYTES, MAX_FUTURE_SKEW_SECS,
};
use validator::Validate;

/// Validates raw batch size BEFORE deserialization.
///
/// Call this first to prevent allocation attacks from oversized payloads.
pub fn validate_batch_size(raw_bytes: &[u8]) -> Result<()> {
    if raw_bytes.len() > MAX_BATCH_SIZE_BYTES {
        return Err(Error::validation(format!(
            "batch {}KB exceeds {}KB limit",
            raw_bytes.len() / 1024,
            MAX_BATCH_SIZE_BYTES / 1024
        )));
    }
    Ok(())
}

/// Validates a single serialized event size.
///
/// Use this to reject oversized events before deserialization.
pub fn validate_event_size(raw_bytes: &[u8]) -> Result<()> {
    if raw_bytes.len() > MAX_EVENT_SIZE_BYTES {
        return Err(Error::validation(format!(
            "event {}KB exceeds {}KB limit",
            raw_bytes.len() / 1024,
            MAX_EVENT_SIZE_BYTES / 1024
        )));
    }
    Ok(())
}

/// Validates an event against its schema.
pub fn validate_event(event: &Event) -> Result<()> {
    // Run validator derive validations
    event
        .validate()
        .map_err(|e| Error::validation(format!("{}", e)))?;

    // Cross-field: reject events claiming to be from the future (allow clock skew)
    let max_future = Duration::seconds(MAX_FUTURE_SKEW_SECS);
    if event.timestamp > event.received_at + max_future {
        return Err(Error::validation(
            "timestamp cannot be more than 5s in the future",
        ));
    }

    // Cross-field: reject stale events older than configured max age
    let max_age = Duration::hours(MAX_EVENT_AGE_HOURS);
    if event.received_at - event.timestamp > max_age {
        return Err(Error::validation(
            "timestamp cannot be more than 24h in the past",
        ));
    }

    // Validate metadata if present
    if let Some(ref meta) = event.metadata {
        meta.validate()
            .map_err(|e| Error::validation(format!("metadata: {}", e)))?;
    }

    // Validate payload-specific rules
    match &event.payload {
        EventPayload::Pageview(data) => {
            data.validate()
                .map_err(|e| Error::validation(format!("pageview: {}", e)))?;
        }
        EventPayload::Click(data) => {
            data.validate()
                .map_err(|e| Error::validation(format!("click: {}", e)))?;
        }
        EventPayload::Scroll(data) => {
            data.validate()
                .map_err(|e| Error::validation(format!("scroll: {}", e)))?;
        }
        EventPayload::Performance(data) => {
            data.metrics
                .validate()
                .map_err(|e| Error::validation(format!("performance: {}", e)))?;
        }
        EventPayload::Custom(data) => {
            data.validate()
                .map_err(|e| Error::validation(format!("custom: {}", e)))?;
        }
    }

    Ok(())
}

/// Validates a batch of events.
pub fn validate_batch(batch: &EventBatch) -> Result<Vec<Error>> {
    batch
        .validate()
        .map_err(|e| Error::validation(format!("batch: {}", e)))?;

    let mut errors = Vec::new();

    for (i, event) in batch.events.iter().enumerate() {
        if let Err(e) = validate_event(event) {
            errors.push(Error::validation(format!("event[{}]: {}", i, e)));
        }
    }

    Ok(errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{PageviewData, EventPayload};
    use uuid::Uuid;

    #[test]
    fn test_valid_pageview_event() {
        let event = Event::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            EventPayload::Pageview(PageviewData {
                title: "Test Page".into(),
                path: "/test".into(),
                load_time: Some(150.0),
                time_to_first_byte: Some(50.0),
                referrer: None,
            }),
        );

        assert!(validate_event(&event).is_ok());
    }
}
