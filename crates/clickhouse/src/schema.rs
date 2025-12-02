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
TTL timestamp + INTERVAL 90 DAY
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
TTL started_at + INTERVAL 90 DAY
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
TTL timestamp + INTERVAL 30 DAY
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
        CREATE_EVENTS_TABLE,
        CREATE_SESSIONS_TABLE,
        CREATE_METRICS_TABLE,
    ]
}

/// Event type values for the type column.
pub mod event_types {
    pub const PAGEVIEW: &str = "pageview";
    pub const PAGELEAVE: &str = "pageleave";
    pub const CLICK: &str = "click";
    pub const SCROLL: &str = "scroll";
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

    /// All valid event types.
    pub const ALL: &[&str] = &[
        PAGEVIEW,
        PAGELEAVE,
        CLICK,
        SCROLL,
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
    ];
}
