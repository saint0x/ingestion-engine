//! Schema validation for events.

use crate::error::{Error, Result};
use crate::events::{Event, EventBatch, EventPayload};
use validator::Validate;

/// Validates an event against its schema.
pub fn validate_event(event: &Event) -> Result<()> {
    // Run validator derive validations
    event
        .validate()
        .map_err(|e| Error::validation(format!("{}", e)))?;

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
        EventPayload::Performance(_) => {
            // No additional validation needed
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
