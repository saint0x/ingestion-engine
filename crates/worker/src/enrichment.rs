//! Event enrichment via user agent parsing.
//!
//! Parses user_agent strings to extract device/browser/OS information
//! in the ConsumerWorker pipeline before ClickHouse insertion.

use engine_core::ClickHouseEvent;
use woothee::parser::Parser;

/// Enrichment worker for parsing user agents.
///
/// Uses the woothee library for fast UA parsing (~6.8us/parse).
/// Extracts: browser name/version, OS, device category.
pub struct EnrichmentWorker {
    parser: Parser,
}

impl EnrichmentWorker {
    /// Creates a new enrichment worker.
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
        }
    }

    /// Enrich a single event by parsing its user_agent.
    ///
    /// Updates device_type, browser, browser_version, and os fields
    /// based on the parsed user agent string.
    pub fn enrich(&self, event: &mut ClickHouseEvent) {
        if event.user_agent.is_empty() {
            return;
        }

        if let Some(result) = self.parser.parse(&event.user_agent) {
            // Browser info
            if !result.name.is_empty() && result.name != "UNKNOWN" {
                event.browser = result.name.to_string();
            }
            if !result.version.is_empty() && result.version != "UNKNOWN" {
                event.browser_version = result.version.to_string();
            }

            // OS info
            if !result.os.is_empty() && result.os != "UNKNOWN" {
                event.os = result.os.to_string();
            }

            // Device type (woothee categories: pc, smartphone, mobilephone, crawler, appliance, misc)
            // Only override if SDK didn't provide a valid device_type
            // SDK-side detection is more accurate for modern iPad/iPhone Safari
            if event.device_type.is_empty() || event.device_type == "unknown" {
                let device_type = match result.category {
                    "pc" => "desktop",
                    "smartphone" => "mobile",
                    "mobilephone" => "mobile",
                    "crawler" => "bot",
                    "appliance" => "other",
                    _ => "unknown",
                };
                event.device_type = device_type.to_string();
            }
        }
    }

    /// Enrich a batch of events in place.
    pub fn enrich_batch(&self, events: &mut [ClickHouseEvent]) {
        for event in events.iter_mut() {
            self.enrich(event);
        }
    }
}

impl Default for EnrichmentWorker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_event(user_agent: &str) -> ClickHouseEvent {
        ClickHouseEvent {
            event_id: "test-123".into(),
            project_id: "proj-1".into(),
            session_id: "sess-1".into(),
            user_id: None,
            event_type: "pageview".into(),
            timestamp: 1704067200000,
            url: "https://example.com".into(),
            path: "/".into(),
            referrer: "".into(),
            user_agent: user_agent.into(),
            device_type: "unknown".into(),
            browser: "unknown".into(),
            browser_version: "unknown".into(),
            os: "unknown".into(),
            country: "unknown".into(),
            region: None,
            city: None,
            data: "{}".into(),
        }
    }

    #[test]
    fn test_chrome_macos() {
        let enricher = EnrichmentWorker::new();
        let mut event = test_event(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
        );

        enricher.enrich(&mut event);

        assert_eq!(event.browser, "Chrome");
        assert!(event.browser_version.starts_with("120"), "Expected version starting with 120, got {}", event.browser_version);
        assert_eq!(event.os, "Mac OSX");
        assert_eq!(event.device_type, "desktop");
    }

    #[test]
    fn test_chrome_windows() {
        let enricher = EnrichmentWorker::new();
        let mut event = test_event(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
        );

        enricher.enrich(&mut event);

        assert_eq!(event.browser, "Chrome");
        assert_eq!(event.os, "Windows 10");
        assert_eq!(event.device_type, "desktop");
    }

    #[test]
    fn test_safari_mobile_iphone() {
        let enricher = EnrichmentWorker::new();
        let mut event = test_event(
            "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1"
        );

        enricher.enrich(&mut event);

        assert_eq!(event.browser, "Safari");
        assert_eq!(event.device_type, "mobile");
    }

    #[test]
    fn test_firefox_linux() {
        let enricher = EnrichmentWorker::new();
        let mut event = test_event(
            "Mozilla/5.0 (X11; Linux x86_64; rv:120.0) Gecko/20100101 Firefox/120.0"
        );

        enricher.enrich(&mut event);

        assert_eq!(event.browser, "Firefox");
        assert!(event.browser_version.starts_with("120"), "Expected version starting with 120, got {}", event.browser_version);
        assert_eq!(event.os, "Linux");
        assert_eq!(event.device_type, "desktop");
    }

    #[test]
    fn test_googlebot() {
        let enricher = EnrichmentWorker::new();
        let mut event = test_event(
            "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)"
        );

        enricher.enrich(&mut event);

        assert_eq!(event.browser, "Googlebot");
        assert_eq!(event.device_type, "bot");
    }

    #[test]
    fn test_empty_user_agent() {
        let enricher = EnrichmentWorker::new();
        let mut event = test_event("");

        enricher.enrich(&mut event);

        // Should remain unchanged
        assert_eq!(event.device_type, "unknown");
        assert_eq!(event.browser, "unknown");
        assert_eq!(event.browser_version, "unknown");
        assert_eq!(event.os, "unknown");
    }

    #[test]
    fn test_unknown_user_agent() {
        let enricher = EnrichmentWorker::new();
        let mut event = test_event("some random string that is not a valid UA");

        enricher.enrich(&mut event);

        // Should remain "unknown" since parsing fails
        assert_eq!(event.device_type, "unknown");
    }

    #[test]
    fn test_batch_enrichment() {
        let enricher = EnrichmentWorker::new();
        let mut events = vec![
            test_event("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120.0.0.0"),
            test_event("Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) Safari/604.1"),
            test_event("Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)"),
        ];

        enricher.enrich_batch(&mut events);

        assert_eq!(events[0].device_type, "desktop");
        assert_eq!(events[1].device_type, "mobile");
        assert_eq!(events[2].device_type, "bot");
    }

    #[test]
    fn test_enrichment_preserves_other_fields() {
        let enricher = EnrichmentWorker::new();
        let mut event = test_event(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
        );
        event.country = "US".to_string();
        event.region = Some("California".to_string());
        event.event_type = "click".to_string();

        enricher.enrich(&mut event);

        // Verify enrichment happened
        assert_eq!(event.browser, "Chrome");
        assert_eq!(event.device_type, "desktop");

        // Verify other fields preserved
        assert_eq!(event.country, "US");
        assert_eq!(event.region, Some("California".to_string()));
        assert_eq!(event.event_type, "click");
        assert_eq!(event.event_id, "test-123");
    }
}
