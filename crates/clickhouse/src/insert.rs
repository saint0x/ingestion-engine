//! Batch insert helpers for ClickHouse.

use crate::client::ClickHouseClient;
use clickhouse::Row;
use engine_core::{ClickHouseEvent, Event, EventPayload, Result};
use serde::{Deserialize, Serialize};
use telemetry::{metrics, MetricsSnapshot};
use tracing::debug;

/// Flattened event row for ClickHouse insertion.
#[derive(Debug, Clone, Row, Serialize)]
pub struct EventRow {
    pub id: String,
    pub tenant_id: String,
    pub session_id: String,
    pub user_id: Option<String>,
    pub event_type: String,
    pub timestamp: i64, // milliseconds since epoch
    pub received_at: i64,

    // Pageview
    pub page_title: Option<String>,
    pub page_path: Option<String>,
    pub load_time: Option<f64>,
    pub time_to_first_byte: Option<f64>,
    pub referrer: Option<String>,

    // Click
    pub click_element: Option<String>,
    pub click_selector: Option<String>,
    pub click_x: Option<f64>,
    pub click_y: Option<f64>,
    pub is_double_click: Option<u8>,
    pub click_text: Option<String>,

    // Scroll
    pub scroll_depth: Option<f64>,
    pub scroll_direction: Option<String>,
    pub scroll_element: Option<String>,

    // Performance
    pub perf_lcp: Option<f64>,
    pub perf_fid: Option<f64>,
    pub perf_cls: Option<f64>,
    pub perf_ttfb: Option<f64>,
    pub perf_fcp: Option<f64>,
    pub perf_dom_content_loaded: Option<f64>,
    pub perf_load_complete: Option<f64>,
    pub perf_resource_count: Option<u32>,
    pub perf_memory_usage: Option<u64>,

    // Custom
    pub custom_event_name: Option<String>,
    pub custom_properties: Option<String>,

    // Metadata
    pub user_agent: Option<String>,
    pub ip: Option<String>,
    pub screen_width: Option<u32>,
    pub screen_height: Option<u32>,
    pub viewport_width: Option<u32>,
    pub viewport_height: Option<u32>,
    pub device_pixel_ratio: Option<f64>,
    pub timezone: Option<String>,
    pub language: Option<String>,
}

impl From<Event> for EventRow {
    fn from(event: Event) -> Self {
        let mut row = EventRow {
            id: event.id.to_string(),
            tenant_id: event.tenant_id.to_string(),
            session_id: event.session_id.to_string(),
            user_id: event.user_id,
            event_type: event.payload.event_type().to_string(),
            timestamp: event.timestamp.timestamp_millis(),
            received_at: event.received_at.timestamp_millis(),

            page_title: None,
            page_path: None,
            load_time: None,
            time_to_first_byte: None,
            referrer: None,

            click_element: None,
            click_selector: None,
            click_x: None,
            click_y: None,
            is_double_click: None,
            click_text: None,

            scroll_depth: None,
            scroll_direction: None,
            scroll_element: None,

            perf_lcp: None,
            perf_fid: None,
            perf_cls: None,
            perf_ttfb: None,
            perf_fcp: None,
            perf_dom_content_loaded: None,
            perf_load_complete: None,
            perf_resource_count: None,
            perf_memory_usage: None,

            custom_event_name: None,
            custom_properties: None,

            user_agent: None,
            ip: None,
            screen_width: None,
            screen_height: None,
            viewport_width: None,
            viewport_height: None,
            device_pixel_ratio: None,
            timezone: None,
            language: None,
        };

        // Populate event-specific fields
        match event.payload {
            EventPayload::Pageview(data) => {
                row.page_title = Some(data.title);
                row.page_path = Some(data.path);
                row.load_time = data.load_time;
                row.time_to_first_byte = data.time_to_first_byte;
                row.referrer = data.referrer;
            }
            EventPayload::Click(data) => {
                row.click_element = Some(data.element);
                row.click_selector = data.selector;
                if let Some(coords) = data.coordinates {
                    row.click_x = Some(coords.x);
                    row.click_y = Some(coords.y);
                }
                row.is_double_click = Some(if data.is_double_click { 1 } else { 0 });
                row.click_text = data.text;
            }
            EventPayload::Scroll(data) => {
                row.scroll_depth = Some(data.scroll_depth);
                row.scroll_direction = Some(format!("{:?}", data.direction).to_lowercase());
                row.scroll_element = data.element;
            }
            EventPayload::Performance(data) => {
                row.perf_lcp = data.metrics.lcp;
                row.perf_fid = data.metrics.fid;
                row.perf_cls = data.metrics.cls;
                row.perf_ttfb = data.metrics.ttfb;
                row.perf_fcp = data.metrics.fcp;
                row.perf_dom_content_loaded = data.metrics.dom_content_loaded;
                row.perf_load_complete = data.metrics.load_complete;
                row.perf_resource_count = data.metrics.resource_count;
                row.perf_memory_usage = data.metrics.memory_usage;
            }
            EventPayload::Custom(data) => {
                row.custom_event_name = Some(data.event_name);
                row.custom_properties = Some(data.properties.to_string());
            }
        }

        // Populate metadata
        if let Some(meta) = event.metadata {
            row.user_agent = meta.user_agent;
            row.ip = meta.ip;
            row.screen_width = meta.screen_width;
            row.screen_height = meta.screen_height;
            row.viewport_width = meta.viewport_width;
            row.viewport_height = meta.viewport_height;
            row.device_pixel_ratio = meta.device_pixel_ratio;
            row.timezone = meta.timezone;
            row.language = meta.language;
        }

        row
    }
}

/// Insert events into ClickHouse (legacy format).
pub async fn insert_events(client: &ClickHouseClient, events: Vec<Event>) -> Result<usize> {
    if events.is_empty() {
        return Ok(0);
    }

    let count = events.len();
    let start = std::time::Instant::now();

    let rows: Vec<EventRow> = events.into_iter().map(EventRow::from).collect();

    let mut insert = client.inner().insert("events")
        .map_err(|e| engine_core::Error::internal(format!("Insert error: {}", e)))?;

    for row in rows {
        insert.write(&row).await
            .map_err(|e| engine_core::Error::internal(format!("Write error: {}", e)))?;
    }

    insert.end().await
        .map_err(|e| engine_core::Error::internal(format!("End error: {}", e)))?;

    let elapsed = start.elapsed();
    metrics().clickhouse_latency_ms.observe(elapsed.as_millis() as u64);
    metrics().clickhouse_inserts.inc();

    debug!(
        count = count,
        latency_ms = %elapsed.as_millis(),
        "Inserted events to ClickHouse"
    );

    Ok(count)
}

/// Row for new ClickHouse events table (overwatch.events).
///
/// Maps to the production schema with project_id, LowCardinality fields,
/// and JSON data blob for extensibility.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct ClickHouseEventRow {
    pub event_id: String,
    pub project_id: String,
    pub session_id: String,
    pub user_id: Option<String>,
    #[serde(rename = "type")]
    pub event_type: String,
    pub timestamp: i64, // DateTime64(3) as milliseconds
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
    pub data: String, // JSON blob
}

impl From<ClickHouseEvent> for ClickHouseEventRow {
    fn from(event: ClickHouseEvent) -> Self {
        Self {
            event_id: event.event_id,
            project_id: event.project_id,
            session_id: event.session_id,
            user_id: event.user_id,
            event_type: event.event_type,
            timestamp: event.timestamp,
            url: event.url,
            path: event.path,
            referrer: event.referrer,
            user_agent: event.user_agent,
            device_type: event.device_type,
            browser: event.browser,
            browser_version: event.browser_version,
            os: event.os,
            country: event.country,
            region: event.region,
            city: event.city,
            data: event.data,
        }
    }
}

/// Insert ClickHouseEvent records from the consumer.
///
/// This is the main insert function for the production pipeline,
/// inserting into the overwatch.events table.
pub async fn insert_clickhouse_events(
    client: &ClickHouseClient,
    events: Vec<ClickHouseEvent>,
) -> Result<usize> {
    if events.is_empty() {
        return Ok(0);
    }

    let count = events.len();
    let start = std::time::Instant::now();

    let rows: Vec<ClickHouseEventRow> = events.into_iter().map(ClickHouseEventRow::from).collect();

    // Insert into overwatch.events table
    let mut insert = client.inner().insert("overwatch.events")
        .map_err(|e| {
            metrics().clickhouse_insert_errors.inc();
            engine_core::Error::internal(format!("Insert error: {}", e))
        })?;

    for row in &rows {
        insert.write(row).await
            .map_err(|e| {
                metrics().clickhouse_insert_errors.inc();
                engine_core::Error::internal(format!("Write error: {}", e))
            })?;
    }

    insert.end().await
        .map_err(|e| {
            metrics().clickhouse_insert_errors.inc();
            engine_core::Error::internal(format!("End error: {}", e))
        })?;

    let elapsed = start.elapsed();
    metrics().batch_insert_latency_ms.observe(elapsed.as_millis() as u64);
    metrics().clickhouse_inserts.inc();
    metrics().events_inserted.inc_by(count as u64);

    debug!(
        count = count,
        latency_ms = %elapsed.as_millis(),
        "Inserted ClickHouse events"
    );

    Ok(count)
}

/// Internal metrics row for ClickHouse.
#[derive(Debug, Clone, Row, Serialize)]
pub struct MetricsRow {
    pub timestamp: i64,
    pub events_received: u64,
    pub events_validated: u64,
    pub events_failed_validation: u64,
    pub batches_received: u64,
    pub batches_sent_to_redpanda: u64,
    pub events_sent_to_redpanda: u64,
    pub redpanda_send_errors: u64,
    pub clickhouse_inserts: u64,
    pub clickhouse_insert_errors: u64,
    pub ingest_latency_mean_ms: f64,
    pub redpanda_latency_mean_ms: f64,
    pub clickhouse_latency_mean_ms: f64,
    pub active_connections: u64,
    pub queue_depth: u64,
    pub backpressure_active: u8,
}

impl From<MetricsSnapshot> for MetricsRow {
    fn from(snapshot: MetricsSnapshot) -> Self {
        Self {
            timestamp: snapshot.timestamp.timestamp_millis(),
            events_received: snapshot.events_received,
            events_validated: snapshot.events_validated,
            events_failed_validation: snapshot.events_failed_validation,
            batches_received: snapshot.batches_received,
            batches_sent_to_redpanda: snapshot.batches_sent_to_redpanda,
            events_sent_to_redpanda: snapshot.events_sent_to_redpanda,
            redpanda_send_errors: snapshot.redpanda_send_errors,
            clickhouse_inserts: snapshot.clickhouse_inserts,
            clickhouse_insert_errors: snapshot.clickhouse_insert_errors,
            ingest_latency_mean_ms: snapshot.ingest_latency_mean_ms,
            redpanda_latency_mean_ms: snapshot.redpanda_latency_mean_ms,
            clickhouse_latency_mean_ms: snapshot.clickhouse_latency_mean_ms,
            active_connections: snapshot.active_connections,
            queue_depth: snapshot.queue_depth,
            backpressure_active: if snapshot.backpressure_active { 1 } else { 0 },
        }
    }
}

/// Insert internal metrics snapshot.
pub async fn insert_metrics(client: &ClickHouseClient, snapshot: MetricsSnapshot) -> Result<()> {
    let row = MetricsRow::from(snapshot);

    let mut insert = client.inner().insert("internal_metrics")
        .map_err(|e| engine_core::Error::internal(format!("Insert error: {}", e)))?;

    insert.write(&row).await
        .map_err(|e| engine_core::Error::internal(format!("Write error: {}", e)))?;

    insert.end().await
        .map_err(|e| engine_core::Error::internal(format!("End error: {}", e)))?;

    Ok(())
}

// ============================================================================
// Specialized table row types for TS daemon compatibility
// ============================================================================

/// Row for overwatch.pageviews table.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct PageviewRow {
    pub project_id: String,
    pub session_id: String,
    pub timestamp: i64,
    pub url: String,
    pub title: String,
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
}

/// Row for overwatch.clicks table.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct ClickRow {
    pub project_id: String,
    pub session_id: String,
    pub timestamp: i64,
    pub x: f64,
    pub y: f64,
    pub target: String,
    pub selector: String,
    pub url: String,
}

/// Row for overwatch.scroll_events table.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct ScrollEventRow {
    pub project_id: String,
    pub session_id: String,
    pub timestamp: i64,
    pub depth: f64,
    pub max_depth: f64,
    pub url: String,
}

/// Row for overwatch.mouse_moves table.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct MouseMoveRow {
    pub project_id: String,
    pub session_id: String,
    pub timestamp: i64,
    pub x: f64,
    pub y: f64,
    pub viewport_x: f64,
    pub viewport_y: f64,
    pub url: String,
}

/// Row for overwatch.form_events table.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct FormEventRow {
    pub project_id: String,
    pub session_id: String,
    pub timestamp: i64,
    pub form_id: String,
    pub field_name: String,
    pub event_type: String,
    pub url: String,
}

/// Row for overwatch.errors table.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct ErrorRow {
    pub project_id: String,
    pub session_id: String,
    pub timestamp: i64,
    pub message: String,
    pub stack: String,
    pub url: String,
    pub line: u32,
    pub column: u32,
}

/// Row for overwatch.performance_metrics table.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct PerformanceMetricRow {
    pub project_id: String,
    pub session_id: String,
    pub timestamp: i64,
    pub lcp: Option<f64>,
    pub fid: Option<f64>,
    pub cls: Option<f64>,
    pub ttfb: Option<f64>,
    pub fcp: Option<f64>,
    pub url: String,
}

/// Row for overwatch.visibility_events table.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct VisibilityEventRow {
    pub project_id: String,
    pub session_id: String,
    pub timestamp: i64,
    pub state: String,
    pub hidden_duration: Option<u64>,
    pub url: String,
}

/// Row for overwatch.resource_loads table.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct ResourceLoadRow {
    pub project_id: String,
    pub session_id: String,
    pub timestamp: i64,
    pub resource_url: String,
    pub resource_type: String,
    pub duration: f64,
    pub size: u64,
    pub url: String,
}

/// Row for overwatch.geographic table.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct GeographicRow {
    pub project_id: String,
    pub session_id: String,
    pub timestamp: i64,
    pub country: String,
    pub region: Option<String>,
    pub city: Option<String>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub url: String,
}

/// Row for overwatch.custom_events table.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct CustomEventRow {
    pub project_id: String,
    pub session_id: String,
    pub timestamp: i64,
    pub name: String,
    pub properties: String,
    pub url: String,
}

// ============================================================================
// Per-table insert functions
// ============================================================================

/// Insert pageview events.
pub async fn insert_pageviews(client: &ClickHouseClient, rows: Vec<PageviewRow>) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let count = rows.len();
    let mut insert = client.inner().insert("overwatch.pageviews")
        .map_err(|e| engine_core::Error::internal(format!("Insert error: {}", e)))?;
    for row in &rows {
        insert.write(row).await
            .map_err(|e| engine_core::Error::internal(format!("Write error: {}", e)))?;
    }
    insert.end().await
        .map_err(|e| engine_core::Error::internal(format!("End error: {}", e)))?;
    Ok(count)
}

/// Insert click events.
pub async fn insert_clicks(client: &ClickHouseClient, rows: Vec<ClickRow>) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let count = rows.len();
    let mut insert = client.inner().insert("overwatch.clicks")
        .map_err(|e| engine_core::Error::internal(format!("Insert error: {}", e)))?;
    for row in &rows {
        insert.write(row).await
            .map_err(|e| engine_core::Error::internal(format!("Write error: {}", e)))?;
    }
    insert.end().await
        .map_err(|e| engine_core::Error::internal(format!("End error: {}", e)))?;
    Ok(count)
}

/// Insert scroll events.
pub async fn insert_scroll_events(client: &ClickHouseClient, rows: Vec<ScrollEventRow>) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let count = rows.len();
    let mut insert = client.inner().insert("overwatch.scroll_events")
        .map_err(|e| engine_core::Error::internal(format!("Insert error: {}", e)))?;
    for row in &rows {
        insert.write(row).await
            .map_err(|e| engine_core::Error::internal(format!("Write error: {}", e)))?;
    }
    insert.end().await
        .map_err(|e| engine_core::Error::internal(format!("End error: {}", e)))?;
    Ok(count)
}

/// Insert mouse move events.
pub async fn insert_mouse_moves(client: &ClickHouseClient, rows: Vec<MouseMoveRow>) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let count = rows.len();
    let mut insert = client.inner().insert("overwatch.mouse_moves")
        .map_err(|e| engine_core::Error::internal(format!("Insert error: {}", e)))?;
    for row in &rows {
        insert.write(row).await
            .map_err(|e| engine_core::Error::internal(format!("Write error: {}", e)))?;
    }
    insert.end().await
        .map_err(|e| engine_core::Error::internal(format!("End error: {}", e)))?;
    Ok(count)
}

/// Insert form events.
pub async fn insert_form_events(client: &ClickHouseClient, rows: Vec<FormEventRow>) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let count = rows.len();
    let mut insert = client.inner().insert("overwatch.form_events")
        .map_err(|e| engine_core::Error::internal(format!("Insert error: {}", e)))?;
    for row in &rows {
        insert.write(row).await
            .map_err(|e| engine_core::Error::internal(format!("Write error: {}", e)))?;
    }
    insert.end().await
        .map_err(|e| engine_core::Error::internal(format!("End error: {}", e)))?;
    Ok(count)
}

/// Insert error events.
pub async fn insert_errors(client: &ClickHouseClient, rows: Vec<ErrorRow>) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let count = rows.len();
    let mut insert = client.inner().insert("overwatch.errors")
        .map_err(|e| engine_core::Error::internal(format!("Insert error: {}", e)))?;
    for row in &rows {
        insert.write(row).await
            .map_err(|e| engine_core::Error::internal(format!("Write error: {}", e)))?;
    }
    insert.end().await
        .map_err(|e| engine_core::Error::internal(format!("End error: {}", e)))?;
    Ok(count)
}

/// Insert performance metric events.
pub async fn insert_performance_metrics(client: &ClickHouseClient, rows: Vec<PerformanceMetricRow>) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let count = rows.len();
    let mut insert = client.inner().insert("overwatch.performance_metrics")
        .map_err(|e| engine_core::Error::internal(format!("Insert error: {}", e)))?;
    for row in &rows {
        insert.write(row).await
            .map_err(|e| engine_core::Error::internal(format!("Write error: {}", e)))?;
    }
    insert.end().await
        .map_err(|e| engine_core::Error::internal(format!("End error: {}", e)))?;
    Ok(count)
}

/// Insert visibility events.
pub async fn insert_visibility_events(client: &ClickHouseClient, rows: Vec<VisibilityEventRow>) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let count = rows.len();
    let mut insert = client.inner().insert("overwatch.visibility_events")
        .map_err(|e| engine_core::Error::internal(format!("Insert error: {}", e)))?;
    for row in &rows {
        insert.write(row).await
            .map_err(|e| engine_core::Error::internal(format!("Write error: {}", e)))?;
    }
    insert.end().await
        .map_err(|e| engine_core::Error::internal(format!("End error: {}", e)))?;
    Ok(count)
}

/// Insert resource load events.
pub async fn insert_resource_loads(client: &ClickHouseClient, rows: Vec<ResourceLoadRow>) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let count = rows.len();
    let mut insert = client.inner().insert("overwatch.resource_loads")
        .map_err(|e| engine_core::Error::internal(format!("Insert error: {}", e)))?;
    for row in &rows {
        insert.write(row).await
            .map_err(|e| engine_core::Error::internal(format!("Write error: {}", e)))?;
    }
    insert.end().await
        .map_err(|e| engine_core::Error::internal(format!("End error: {}", e)))?;
    Ok(count)
}

/// Insert geographic events.
pub async fn insert_geographic(client: &ClickHouseClient, rows: Vec<GeographicRow>) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let count = rows.len();
    let mut insert = client.inner().insert("overwatch.geographic")
        .map_err(|e| engine_core::Error::internal(format!("Insert error: {}", e)))?;
    for row in &rows {
        insert.write(row).await
            .map_err(|e| engine_core::Error::internal(format!("Write error: {}", e)))?;
    }
    insert.end().await
        .map_err(|e| engine_core::Error::internal(format!("End error: {}", e)))?;
    Ok(count)
}

/// Insert custom events.
pub async fn insert_custom_events(client: &ClickHouseClient, rows: Vec<CustomEventRow>) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0);
    }
    let count = rows.len();
    let mut insert = client.inner().insert("overwatch.custom_events")
        .map_err(|e| engine_core::Error::internal(format!("Insert error: {}", e)))?;
    for row in &rows {
        insert.write(row).await
            .map_err(|e| engine_core::Error::internal(format!("Write error: {}", e)))?;
    }
    insert.end().await
        .map_err(|e| engine_core::Error::internal(format!("End error: {}", e)))?;
    Ok(count)
}
