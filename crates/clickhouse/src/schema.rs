//! ClickHouse table schemas.
//!
//! Schema follows the production spec with:
//! - project_id instead of tenant_id
//! - LowCardinality for enum-like fields
//! - DateTime64(3) for millisecond precision
//! - JSON data blob for extensibility

/// SQL for creating the events table.
///
/// This is the main events table that stores all analytics events
/// after transformation from SDK format.
pub const CREATE_EVENTS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS overwatch.events (
    -- Core identifiers
    event_id String,
    project_id String,
    session_id String,
    user_id Nullable(String),

    -- Event classification
    type LowCardinality(String),
    timestamp DateTime64(3),

    -- Page information
    url String,
    path String,
    referrer String,

    -- Client information
    user_agent String,
    device_type LowCardinality(String),
    browser LowCardinality(String),
    browser_version String,
    os LowCardinality(String),

    -- Location (from geo-enrichment)
    country LowCardinality(String),
    region Nullable(String),
    city Nullable(String),

    -- Extensible JSON data blob for event-specific fields
    data String,

    -- Metadata
    created_at DateTime DEFAULT now()
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (project_id, timestamp, event_id)
TTL toDateTime(timestamp) + INTERVAL 90 DAY
SETTINGS index_granularity = 8192
"#;

/// SQL for creating the sessions table.
///
/// Aggregated session data computed by background workers.
pub const CREATE_SESSIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS overwatch.sessions (
    session_id String,
    project_id String,
    user_id Nullable(String),
    started_at DateTime64(3),
    ended_at Nullable(DateTime64(3)),
    event_count UInt64,
    duration_ms Nullable(UInt64),

    -- First/last event info
    entry_url String,
    entry_path String,
    exit_url Nullable(String),
    exit_path Nullable(String),
    referrer String,

    -- Aggregates
    pageview_count UInt32,
    click_count UInt32,
    scroll_max_depth Nullable(Float64),

    -- Client info (from first event)
    device_type LowCardinality(String),
    browser LowCardinality(String),
    os LowCardinality(String),
    country LowCardinality(String),

    -- Metadata
    created_at DateTime DEFAULT now(),
    updated_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(updated_at)
PARTITION BY toYYYYMM(started_at)
ORDER BY (project_id, session_id)
TTL toDateTime(started_at) + INTERVAL 90 DAY
SETTINGS index_granularity = 8192
"#;

/// SQL for creating the internal metrics table (dogfooding).
///
/// Stores system metrics for monitoring the ingestion engine itself.
pub const CREATE_METRICS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS overwatch.internal_metrics (
    timestamp DateTime64(3),
    events_received UInt64,
    events_validated UInt64,
    events_failed_validation UInt64,
    batches_received UInt64,
    batches_sent_to_redpanda UInt64,
    events_sent_to_redpanda UInt64,
    redpanda_send_errors UInt64,
    clickhouse_inserts UInt64,
    clickhouse_insert_errors UInt64,
    ingest_latency_mean_ms Float64,
    redpanda_latency_mean_ms Float64,
    clickhouse_latency_mean_ms Float64,
    active_connections UInt64,
    queue_depth UInt64,
    backpressure_active UInt8
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY timestamp
TTL toDateTime(timestamp) + INTERVAL 30 DAY
SETTINGS index_granularity = 8192
"#;

/// SQL for creating the pageviews table.
pub const CREATE_PAGEVIEWS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS overwatch.pageviews (
    project_id String,
    session_id String,
    timestamp DateTime64(3),
    -- Location
    url String,
    path String,
    title Nullable(String),
    referrer Nullable(String),
    -- Client info
    user_agent String,
    device_type LowCardinality(String),
    browser LowCardinality(String),
    browser_version String,
    os LowCardinality(String),
    -- Geo
    country LowCardinality(String),
    region Nullable(String),
    city Nullable(String),
    -- Engagement metrics (enrichment)
    time_on_page_seconds Nullable(UInt32),
    scroll_depth_percentage UInt8 DEFAULT 0,
    page_load_time_ms Nullable(UInt32)
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (project_id, timestamp)
TTL toDateTime(timestamp) + INTERVAL 90 DAY
SETTINGS index_granularity = 8192
"#;

/// SQL for creating the clicks table.
pub const CREATE_CLICKS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS overwatch.clicks (
    project_id String,
    session_id String,
    timestamp DateTime64(3),
    url String,
    x Float64,
    y Float64,
    -- Element info
    selector Nullable(String),
    target Nullable(String),
    element_text Nullable(String),
    element_tag Nullable(String),
    element_id Nullable(String),
    element_class Nullable(String),
    -- Viewport
    viewport_width Nullable(UInt16),
    viewport_height Nullable(UInt16)
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (project_id, timestamp)
TTL toDateTime(timestamp) + INTERVAL 90 DAY
SETTINGS index_granularity = 8192
"#;

/// SQL for creating the scroll_events table.
pub const CREATE_SCROLL_EVENTS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS overwatch.scroll_events (
    project_id String,
    session_id String,
    timestamp DateTime64(3),
    depth Float64,
    max_depth Float64,
    url String
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (project_id, timestamp)
TTL toDateTime(timestamp) + INTERVAL 90 DAY
SETTINGS index_granularity = 8192
"#;

/// SQL for creating the mouse_moves table.
pub const CREATE_MOUSE_MOVES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS overwatch.mouse_moves (
    project_id String,
    session_id String,
    timestamp DateTime64(3),
    x Float64,
    y Float64,
    viewport_x Float64,
    viewport_y Float64,
    url String
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (project_id, timestamp)
TTL toDateTime(timestamp) + INTERVAL 90 DAY
SETTINGS index_granularity = 8192
"#;

/// SQL for creating the form_events table.
pub const CREATE_FORM_EVENTS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS overwatch.form_events (
    project_id String,
    session_id String,
    timestamp DateTime64(3),
    form_id String,
    field_name String,
    event_type LowCardinality(String),
    url String
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (project_id, timestamp)
TTL toDateTime(timestamp) + INTERVAL 90 DAY
SETTINGS index_granularity = 8192
"#;

/// SQL for creating the errors table.
pub const CREATE_ERRORS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS overwatch.errors (
    project_id String,
    session_id String,
    timestamp DateTime64(3),
    message String,
    stack String,
    url String,
    line UInt32,
    column UInt32
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (project_id, timestamp)
TTL toDateTime(timestamp) + INTERVAL 90 DAY
SETTINGS index_granularity = 8192
"#;

/// SQL for creating the performance_metrics table.
pub const CREATE_PERFORMANCE_METRICS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS overwatch.performance_metrics (
    project_id String,
    session_id String,
    timestamp DateTime64(3),
    lcp Nullable(Float64),
    fid Nullable(Float64),
    cls Nullable(Float64),
    ttfb Nullable(Float64),
    fcp Nullable(Float64),
    url String
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (project_id, timestamp)
TTL toDateTime(timestamp) + INTERVAL 90 DAY
SETTINGS index_granularity = 8192
"#;

/// SQL for creating the visibility_events table.
pub const CREATE_VISIBILITY_EVENTS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS overwatch.visibility_events (
    project_id String,
    session_id String,
    timestamp DateTime64(3),
    state LowCardinality(String),
    hidden_duration Nullable(UInt64),
    url String
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (project_id, timestamp)
TTL toDateTime(timestamp) + INTERVAL 90 DAY
SETTINGS index_granularity = 8192
"#;

/// SQL for creating the resource_loads table.
pub const CREATE_RESOURCE_LOADS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS overwatch.resource_loads (
    project_id String,
    session_id String,
    timestamp DateTime64(3),
    resource_url String,
    resource_type LowCardinality(String),
    duration Float64,
    size UInt64,
    url String
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (project_id, timestamp)
TTL toDateTime(timestamp) + INTERVAL 90 DAY
SETTINGS index_granularity = 8192
"#;

/// SQL for creating the geographic table.
pub const CREATE_GEOGRAPHIC_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS overwatch.geographic (
    project_id String,
    session_id String,
    timestamp DateTime64(3),
    country LowCardinality(String),
    region Nullable(String),
    city Nullable(String),
    lat Nullable(Float64),
    lng Nullable(Float64),
    url String
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (project_id, timestamp)
TTL toDateTime(timestamp) + INTERVAL 90 DAY
SETTINGS index_granularity = 8192
"#;

/// SQL for creating the custom_events table.
pub const CREATE_CUSTOM_EVENTS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS overwatch.custom_events (
    project_id String,
    session_id String,
    timestamp DateTime64(3),
    name String,
    properties String,
    url String
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (project_id, timestamp)
TTL toDateTime(timestamp) + INTERVAL 90 DAY
SETTINGS index_granularity = 8192
"#;

/// SQL for creating the database.
pub const CREATE_DATABASE: &str = r#"
CREATE DATABASE IF NOT EXISTS overwatch
"#;

/// All table creation statements.
pub fn all_tables() -> Vec<&'static str> {
    vec![
        CREATE_DATABASE,
        // Legacy unified events table (kept for backwards compatibility)
        CREATE_EVENTS_TABLE,
        CREATE_SESSIONS_TABLE,
        CREATE_METRICS_TABLE,
        // Specialized tables for TS daemon compatibility
        CREATE_PAGEVIEWS_TABLE,
        CREATE_CLICKS_TABLE,
        CREATE_SCROLL_EVENTS_TABLE,
        CREATE_MOUSE_MOVES_TABLE,
        CREATE_FORM_EVENTS_TABLE,
        CREATE_ERRORS_TABLE,
        CREATE_PERFORMANCE_METRICS_TABLE,
        CREATE_VISIBILITY_EVENTS_TABLE,
        CREATE_RESOURCE_LOADS_TABLE,
        CREATE_GEOGRAPHIC_TABLE,
        CREATE_CUSTOM_EVENTS_TABLE,
    ]
}

use crate::client::ClickHouseClient;
use engine_core::Result;

/// Initialize the database schema.
///
/// Creates the database and all tables if they don't exist.
pub async fn init_schema(client: &ClickHouseClient) -> Result<()> {
    for sql in all_tables() {
        client
            .inner()
            .query(sql)
            .execute()
            .await
            .map_err(|e| engine_core::Error::internal(format!("Schema init error: {}", e)))?;
    }
    Ok(())
}

/// Event type values for the type column.
pub mod event_types {
    // Core analytics events
    pub const PAGEVIEW: &str = "pageview";
    pub const PAGELEAVE: &str = "pageleave";
    pub const CLICK: &str = "click";
    pub const SCROLL: &str = "scroll";
    pub const MOUSE_MOVE: &str = "mouse_move";
    pub const FORM_FOCUS: &str = "form_focus";
    pub const FORM_BLUR: &str = "form_blur";
    pub const FORM_SUBMIT: &str = "form_submit";
    pub const FORM_ABANDON: &str = "form_abandon";
    pub const ERROR: &str = "error";
    pub const VISIBILITY_CHANGE: &str = "visibility_change";
    pub const RESOURCE_LOAD: &str = "resource_load";
    pub const SESSION_START: &str = "session_start";
    pub const SESSION_END: &str = "session_end";
    pub const PERFORMANCE: &str = "performance";
    pub const CUSTOM: &str = "custom";

    // Overwatch Triggers v1.0 - Context-based notification system
    pub const EXIT_INTENT: &str = "exit_intent";
    pub const IDLE_START: &str = "idle_start";
    pub const IDLE_END: &str = "idle_end";
    pub const ENGAGEMENT_SNAPSHOT: &str = "engagement_snapshot";
    pub const TRIGGER_REGISTERED: &str = "trigger_registered";
    pub const TRIGGER_FIRED: &str = "trigger_fired";
    pub const TRIGGER_DISMISSED: &str = "trigger_dismissed";
    pub const TRIGGER_ACTION: &str = "trigger_action";
    pub const TRIGGER_ERROR: &str = "trigger_error";

    /// All valid event types.
    pub const ALL: &[&str] = &[
        // Core analytics
        PAGEVIEW,
        PAGELEAVE,
        CLICK,
        SCROLL,
        MOUSE_MOVE,
        FORM_FOCUS,
        FORM_BLUR,
        FORM_SUBMIT,
        FORM_ABANDON,
        ERROR,
        VISIBILITY_CHANGE,
        RESOURCE_LOAD,
        SESSION_START,
        SESSION_END,
        PERFORMANCE,
        CUSTOM,
        // Overwatch Triggers
        EXIT_INTENT,
        IDLE_START,
        IDLE_END,
        ENGAGEMENT_SNAPSHOT,
        TRIGGER_REGISTERED,
        TRIGGER_FIRED,
        TRIGGER_DISMISSED,
        TRIGGER_ACTION,
        TRIGGER_ERROR,
    ];

    /// Overwatch Trigger event types only.
    pub const TRIGGER_EVENTS: &[&str] = &[
        EXIT_INTENT,
        IDLE_START,
        IDLE_END,
        ENGAGEMENT_SNAPSHOT,
        TRIGGER_REGISTERED,
        TRIGGER_FIRED,
        TRIGGER_DISMISSED,
        TRIGGER_ACTION,
        TRIGGER_ERROR,
    ];

    /// High-volume event types that may need sampling.
    pub const HIGH_VOLUME: &[&str] = &[MOUSE_MOVE, ENGAGEMENT_SNAPSHOT];
}
