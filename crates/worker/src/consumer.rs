//! Consumer worker for reading events from Redpanda and inserting to ClickHouse.
//!
//! This worker implements the core data pipeline:
//! 1. Fetch batch of events from Redpanda
//! 2. Enrich events (UA parsing)
//! 3. Route events to specialized tables by type
//! 4. Commit offset (at-least-once delivery)
//! 5. Repeat

use crate::enrichment::EnrichmentWorker;
use clickhouse_client::insert::{
    ClickRow, CustomEventRow, ErrorRow, FormEventRow, GeographicRow, MouseMoveRow,
    PageviewRow, PerformanceMetricRow, ResourceLoadRow, ScrollEventRow, VisibilityEventRow,
};
use clickhouse_client::ClickHouseClient;
use engine_core::{ClickHouseEvent, Result};
use redpanda::Consumer;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Consumer worker configuration.
#[derive(Debug, Clone)]
pub struct ConsumerWorkerConfig {
    /// Maximum retries for ClickHouse insert failures
    pub max_retries: u32,
    /// Backoff between retries
    pub retry_backoff: Duration,
    /// Whether to continue on insert failure (skip batch)
    pub skip_on_failure: bool,
}

impl Default for ConsumerWorkerConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retry_backoff: Duration::from_millis(100),
            skip_on_failure: true,
        }
    }
}

/// Worker that consumes events from Redpanda and inserts to ClickHouse.
pub struct ConsumerWorker {
    consumer: Arc<Consumer>,
    clickhouse: Arc<ClickHouseClient>,
    config: ConsumerWorkerConfig,
    enrichment: EnrichmentWorker,
}

impl ConsumerWorker {
    /// Creates a new consumer worker.
    pub fn new(
        consumer: Arc<Consumer>,
        clickhouse: Arc<ClickHouseClient>,
    ) -> Self {
        Self {
            consumer,
            clickhouse,
            config: ConsumerWorkerConfig::default(),
            enrichment: EnrichmentWorker::new(),
        }
    }

    /// Creates a new consumer worker with custom config.
    pub fn with_config(
        consumer: Arc<Consumer>,
        clickhouse: Arc<ClickHouseClient>,
        config: ConsumerWorkerConfig,
    ) -> Self {
        Self {
            consumer,
            clickhouse,
            config,
            enrichment: EnrichmentWorker::new(),
        }
    }

    /// Main run loop - fetch, insert, commit.
    ///
    /// This runs indefinitely, processing batches of events.
    pub async fn run(&self) -> Result<()> {
        info!(
            topic = %self.consumer.config().topic,
            group_id = %self.consumer.config().group_id,
            batch_size = self.consumer.config().batch_size,
            "Consumer worker starting"
        );

        loop {
            match self.process_batch().await {
                Ok(count) => {
                    if count > 0 {
                        debug!(count = count, "Processed batch");
                    }
                }
                Err(e) => {
                    error!("Batch processing error: {}", e);
                    // Brief pause before retrying
                    tokio::time::sleep(Duration::from_secs(1)).await;

                    // Reset connection on error
                    self.consumer.reset_connection().await;
                }
            }
        }
    }

    /// Processes a single batch: fetch → insert → commit.
    async fn process_batch(&self) -> Result<usize> {
        // 1. Fetch batch from Redpanda
        let (events, offset) = self.consumer.fetch_batch().await?;

        if events.is_empty() {
            return Ok(0);
        }

        let count = events.len();

        // 2. Insert to ClickHouse with retries
        let insert_result = self.insert_with_retry(events).await;

        match insert_result {
            Ok(inserted) => {
                // 3. Commit offset after successful insert
                if let Some(offset) = offset {
                    self.consumer.commit(offset).await?;
                }

                Ok(inserted)
            }
            Err(e) => {
                error!(
                    count = count,
                    error = %e,
                    "Failed to insert batch after retries"
                );

                if self.config.skip_on_failure {
                    // Skip this batch and commit anyway to avoid infinite retry
                    warn!("Skipping failed batch, committing offset");
                    if let Some(offset) = offset {
                        self.consumer.commit(offset).await?;
                    }
                    Ok(0)
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Process a single batch (public method for testing).
    ///
    /// Returns the number of events inserted, or 0 if batch was empty.
    pub async fn process_one_batch(&self) -> Result<usize> {
        self.process_batch().await
    }

    /// Inserts events with retry logic.
    ///
    /// Events are enriched (UA parsing) before insertion, then routed to specialized tables.
    async fn insert_with_retry(
        &self,
        events: Vec<ClickHouseEvent>,
    ) -> Result<usize> {
        // Enrich events before insertion
        let mut events = events;
        self.enrichment.enrich_batch(&mut events);

        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                let backoff = self.config.retry_backoff * attempt;
                warn!(
                    attempt = attempt,
                    backoff_ms = %backoff.as_millis(),
                    "Retrying ClickHouse insert"
                );
                tokio::time::sleep(backoff).await;
            }

            // Route events to specialized tables
            match self.route_and_insert(&events).await {
                Ok(count) => return Ok(count),
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            engine_core::Error::internal("Insert failed with unknown error")
        }))
    }

    /// Routes events to specialized tables based on event type.
    ///
    /// Each event type is inserted into its corresponding table:
    /// - pageview, pageleave → pageviews
    /// - click → clicks
    /// - scroll → scroll_events
    /// - mouse_move → mouse_moves
    /// - form_focus, form_blur, form_submit, form_abandon → form_events
    /// - error → errors
    /// - performance → performance_metrics
    /// - visibility_change → visibility_events
    /// - resource_load → resource_loads
    /// - custom → custom_events
    /// - session_start, session_end → events (catch-all)
    async fn route_and_insert(&self, events: &[ClickHouseEvent]) -> Result<usize> {
        // Group events by type
        let mut grouped: HashMap<&str, Vec<&ClickHouseEvent>> = HashMap::new();
        for event in events {
            grouped.entry(event.event_type.as_str()).or_default().push(event);
        }

        let mut total_inserted = 0;

        // Process each group
        for (event_type, events) in grouped {
            let count = match event_type {
                "pageview" | "pageleave" => {
                    let rows: Vec<PageviewRow> = events.iter().map(|e| PageviewRow {
                        project_id: e.project_id.clone(),
                        session_id: e.session_id.clone(),
                        timestamp: e.timestamp,
                        url: e.url.clone(),
                        title: extract_title_from_data(&e.data),
                        path: e.path.clone(),
                        referrer: e.referrer.clone(),
                        user_agent: e.user_agent.clone(),
                        device_type: e.device_type.clone(),
                        browser: e.browser.clone(),
                        browser_version: e.browser_version.clone(),
                        os: e.os.clone(),
                        country: e.country.clone(),
                        region: e.region.clone(),
                        city: e.city.clone(),
                    }).collect();
                    clickhouse_client::insert::insert_pageviews(&self.clickhouse, rows).await?
                }
                "click" => {
                    let rows: Vec<ClickRow> = events.iter().map(|e| {
                        let (x, y, target, selector) = extract_click_data(&e.data);
                        ClickRow {
                            project_id: e.project_id.clone(),
                            session_id: e.session_id.clone(),
                            timestamp: e.timestamp,
                            x,
                            y,
                            target,
                            selector,
                            url: e.url.clone(),
                        }
                    }).collect();
                    clickhouse_client::insert::insert_clicks(&self.clickhouse, rows).await?
                }
                "scroll" => {
                    let rows: Vec<ScrollEventRow> = events.iter().map(|e| {
                        let (depth, max_depth) = extract_scroll_data(&e.data);
                        ScrollEventRow {
                            project_id: e.project_id.clone(),
                            session_id: e.session_id.clone(),
                            timestamp: e.timestamp,
                            depth,
                            max_depth,
                            url: e.url.clone(),
                        }
                    }).collect();
                    clickhouse_client::insert::insert_scroll_events(&self.clickhouse, rows).await?
                }
                "mouse_move" => {
                    let rows: Vec<MouseMoveRow> = events.iter().map(|e| {
                        let (x, y, vx, vy) = extract_mouse_move_data(&e.data);
                        MouseMoveRow {
                            project_id: e.project_id.clone(),
                            session_id: e.session_id.clone(),
                            timestamp: e.timestamp,
                            x,
                            y,
                            viewport_x: vx,
                            viewport_y: vy,
                            url: e.url.clone(),
                        }
                    }).collect();
                    clickhouse_client::insert::insert_mouse_moves(&self.clickhouse, rows).await?
                }
                "form_focus" | "form_blur" | "form_submit" | "form_abandon" => {
                    let rows: Vec<FormEventRow> = events.iter().map(|e| {
                        let (form_id, field_name) = extract_form_data(&e.data);
                        FormEventRow {
                            project_id: e.project_id.clone(),
                            session_id: e.session_id.clone(),
                            timestamp: e.timestamp,
                            form_id,
                            field_name,
                            event_type: e.event_type.clone(),
                            url: e.url.clone(),
                        }
                    }).collect();
                    clickhouse_client::insert::insert_form_events(&self.clickhouse, rows).await?
                }
                "error" => {
                    let rows: Vec<ErrorRow> = events.iter().map(|e| {
                        let (message, stack, line, column) = extract_error_data(&e.data);
                        ErrorRow {
                            project_id: e.project_id.clone(),
                            session_id: e.session_id.clone(),
                            timestamp: e.timestamp,
                            message,
                            stack,
                            url: e.url.clone(),
                            line,
                            column,
                        }
                    }).collect();
                    clickhouse_client::insert::insert_errors(&self.clickhouse, rows).await?
                }
                "performance" => {
                    let rows: Vec<PerformanceMetricRow> = events.iter().map(|e| {
                        let (lcp, fid, cls, ttfb, fcp) = extract_performance_data(&e.data);
                        PerformanceMetricRow {
                            project_id: e.project_id.clone(),
                            session_id: e.session_id.clone(),
                            timestamp: e.timestamp,
                            lcp,
                            fid,
                            cls,
                            ttfb,
                            fcp,
                            url: e.url.clone(),
                        }
                    }).collect();
                    clickhouse_client::insert::insert_performance_metrics(&self.clickhouse, rows).await?
                }
                "visibility_change" => {
                    let rows: Vec<VisibilityEventRow> = events.iter().map(|e| {
                        let (state, hidden_duration) = extract_visibility_data(&e.data);
                        VisibilityEventRow {
                            project_id: e.project_id.clone(),
                            session_id: e.session_id.clone(),
                            timestamp: e.timestamp,
                            state,
                            hidden_duration,
                            url: e.url.clone(),
                        }
                    }).collect();
                    clickhouse_client::insert::insert_visibility_events(&self.clickhouse, rows).await?
                }
                "resource_load" => {
                    let rows: Vec<ResourceLoadRow> = events.iter().map(|e| {
                        let (resource_url, resource_type, duration, size) = extract_resource_data(&e.data);
                        ResourceLoadRow {
                            project_id: e.project_id.clone(),
                            session_id: e.session_id.clone(),
                            timestamp: e.timestamp,
                            resource_url,
                            resource_type,
                            duration,
                            size,
                            url: e.url.clone(),
                        }
                    }).collect();
                    clickhouse_client::insert::insert_resource_loads(&self.clickhouse, rows).await?
                }
                "custom" => {
                    let rows: Vec<CustomEventRow> = events.iter().map(|e| {
                        let (name, properties) = extract_custom_data(&e.data);
                        CustomEventRow {
                            project_id: e.project_id.clone(),
                            session_id: e.session_id.clone(),
                            timestamp: e.timestamp,
                            name,
                            properties,
                            url: e.url.clone(),
                        }
                    }).collect();
                    clickhouse_client::insert::insert_custom_events(&self.clickhouse, rows).await?
                }
                "geographic" => {
                    let rows: Vec<GeographicRow> = events.iter().map(|e| {
                        let (country, region, city, lat, lng) = extract_geographic_data(&e.data);
                        GeographicRow {
                            project_id: e.project_id.clone(),
                            session_id: e.session_id.clone(),
                            timestamp: e.timestamp,
                            country,
                            region,
                            city,
                            lat,
                            lng,
                            url: e.url.clone(),
                        }
                    }).collect();
                    clickhouse_client::insert::insert_geographic(&self.clickhouse, rows).await?
                }
                // session_start, session_end, and any unknown types go to catch-all events table
                _ => {
                    let events_vec: Vec<ClickHouseEvent> = events.iter().map(|e| (*e).clone()).collect();
                    clickhouse_client::insert::insert_clickhouse_events(&self.clickhouse, events_vec).await?
                }
            };
            total_inserted += count;
        }

        Ok(total_inserted)
    }
}

// ============================================================================
// Helper functions to extract typed data from JSON data blob
// ============================================================================

fn extract_title_from_data(data: &str) -> String {
    serde_json::from_str::<serde_json::Value>(data)
        .ok()
        .and_then(|v| v.get("title").and_then(|t| t.as_str()).map(String::from))
        .unwrap_or_default()
}

fn extract_click_data(data: &str) -> (f64, f64, String, String) {
    let v: serde_json::Value = serde_json::from_str(data).unwrap_or_default();
    let x = v.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let y = v.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let target = v.get("target").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let selector = v.get("selector").and_then(|v| v.as_str()).unwrap_or("").to_string();
    (x, y, target, selector)
}

fn extract_scroll_data(data: &str) -> (f64, f64) {
    let v: serde_json::Value = serde_json::from_str(data).unwrap_or_default();
    let depth = v.get("depth").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let max_depth = v.get("maxDepth").or_else(|| v.get("max_depth"))
        .and_then(|v| v.as_f64()).unwrap_or(depth);
    (depth, max_depth)
}

fn extract_mouse_move_data(data: &str) -> (f64, f64, f64, f64) {
    let v: serde_json::Value = serde_json::from_str(data).unwrap_or_default();
    let x = v.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let y = v.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let vx = v.get("viewportX").or_else(|| v.get("viewport_x"))
        .and_then(|v| v.as_f64()).unwrap_or(0.0);
    let vy = v.get("viewportY").or_else(|| v.get("viewport_y"))
        .and_then(|v| v.as_f64()).unwrap_or(0.0);
    (x, y, vx, vy)
}

fn extract_form_data(data: &str) -> (String, String) {
    let v: serde_json::Value = serde_json::from_str(data).unwrap_or_default();
    let form_id = v.get("formId").or_else(|| v.get("form_id"))
        .and_then(|v| v.as_str()).unwrap_or("").to_string();
    let field_name = v.get("fieldName").or_else(|| v.get("field_name"))
        .and_then(|v| v.as_str()).unwrap_or("").to_string();
    (form_id, field_name)
}

fn extract_error_data(data: &str) -> (String, String, u32, u32) {
    let v: serde_json::Value = serde_json::from_str(data).unwrap_or_default();
    let message = v.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let stack = v.get("stack").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let line = v.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let column = v.get("column").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    (message, stack, line, column)
}

fn extract_performance_data(data: &str) -> (Option<f64>, Option<f64>, Option<f64>, Option<f64>, Option<f64>) {
    let v: serde_json::Value = serde_json::from_str(data).unwrap_or_default();
    let lcp = v.get("lcp").and_then(|v| v.as_f64());
    let fid = v.get("fid").and_then(|v| v.as_f64());
    let cls = v.get("cls").and_then(|v| v.as_f64());
    let ttfb = v.get("ttfb").and_then(|v| v.as_f64());
    let fcp = v.get("fcp").and_then(|v| v.as_f64());
    (lcp, fid, cls, ttfb, fcp)
}

fn extract_visibility_data(data: &str) -> (String, Option<u64>) {
    let v: serde_json::Value = serde_json::from_str(data).unwrap_or_default();
    let state = v.get("state").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    let hidden_duration = v.get("hiddenDuration").or_else(|| v.get("hidden_duration"))
        .and_then(|v| v.as_u64());
    (state, hidden_duration)
}

fn extract_resource_data(data: &str) -> (String, String, f64, u64) {
    let v: serde_json::Value = serde_json::from_str(data).unwrap_or_default();
    let resource_url = v.get("resourceUrl").or_else(|| v.get("resource_url"))
        .and_then(|v| v.as_str()).unwrap_or("").to_string();
    let resource_type = v.get("resourceType").or_else(|| v.get("resource_type"))
        .and_then(|v| v.as_str()).unwrap_or("").to_string();
    let duration = v.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let size = v.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
    (resource_url, resource_type, duration, size)
}

fn extract_custom_data(data: &str) -> (String, String) {
    let v: serde_json::Value = serde_json::from_str(data).unwrap_or_default();
    let name = v.get("name").or_else(|| v.get("eventName")).or_else(|| v.get("event_name"))
        .and_then(|v| v.as_str()).unwrap_or("").to_string();
    let properties = v.get("properties")
        .map(|p| p.to_string())
        .unwrap_or_else(|| "{}".to_string());
    (name, properties)
}

fn extract_geographic_data(data: &str) -> (String, Option<String>, Option<String>, Option<f64>, Option<f64>) {
    let v: serde_json::Value = serde_json::from_str(data).unwrap_or_default();
    let country = v.get("country").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    let region = v.get("region").and_then(|v| v.as_str()).map(String::from);
    let city = v.get("city").and_then(|v| v.as_str()).map(String::from);
    let lat = v.get("lat").and_then(|v| v.as_f64());
    let lng = v.get("lng").and_then(|v| v.as_f64());
    (country, region, city, lat, lng)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consumer_worker_config_defaults() {
        let config = ConsumerWorkerConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_backoff, Duration::from_millis(100));
        assert!(config.skip_on_failure);
    }

    #[test]
    fn test_extract_geographic_data() {
        let data = r#"{"country":"US","region":"CA","city":"San Francisco","lat":37.7749,"lng":-122.4194}"#;
        let (country, region, city, lat, lng) = extract_geographic_data(data);
        assert_eq!(country, "US");
        assert_eq!(region, Some("CA".to_string()));
        assert_eq!(city, Some("San Francisco".to_string()));
        assert_eq!(lat, Some(37.7749));
        assert_eq!(lng, Some(-122.4194));
    }

    #[test]
    fn test_extract_geographic_data_minimal() {
        let data = r#"{"country":"DE"}"#;
        let (country, region, city, lat, lng) = extract_geographic_data(data);
        assert_eq!(country, "DE");
        assert_eq!(region, None);
        assert_eq!(city, None);
        assert_eq!(lat, None);
        assert_eq!(lng, None);
    }
}
