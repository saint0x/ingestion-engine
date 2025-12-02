//! ClickHouse table schemas.

/// SQL for creating the events table.
pub const CREATE_EVENTS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS events (
    id UUID,
    tenant_id UUID,
    session_id UUID,
    user_id Nullable(String),
    event_type LowCardinality(String),
    timestamp DateTime64(3),
    received_at DateTime64(3),

    -- Pageview fields
    page_title Nullable(String),
    page_path Nullable(String),
    load_time Nullable(Float64),
    time_to_first_byte Nullable(Float64),
    referrer Nullable(String),

    -- Click fields
    click_element Nullable(String),
    click_selector Nullable(String),
    click_x Nullable(Float64),
    click_y Nullable(Float64),
    is_double_click Nullable(UInt8),
    click_text Nullable(String),

    -- Scroll fields
    scroll_depth Nullable(Float64),
    scroll_direction Nullable(String),
    scroll_element Nullable(String),

    -- Performance fields
    perf_lcp Nullable(Float64),
    perf_fid Nullable(Float64),
    perf_cls Nullable(Float64),
    perf_ttfb Nullable(Float64),
    perf_dom_content_loaded Nullable(Float64),
    perf_load_complete Nullable(Float64),
    perf_resource_count Nullable(UInt32),
    perf_memory_usage Nullable(UInt64),

    -- Custom fields
    custom_event_name Nullable(String),
    custom_properties Nullable(String),

    -- Metadata
    user_agent Nullable(String),
    ip Nullable(String),
    screen_width Nullable(UInt32),
    screen_height Nullable(UInt32),
    viewport_width Nullable(UInt32),
    viewport_height Nullable(UInt32),
    device_pixel_ratio Nullable(Float64),
    timezone Nullable(String),
    language Nullable(String),

    -- Partitioning
    event_date Date DEFAULT toDate(timestamp)
)
ENGINE = MergeTree()
PARTITION BY (tenant_id, toYYYYMM(event_date))
ORDER BY (tenant_id, session_id, timestamp)
TTL event_date + INTERVAL 90 DAY
SETTINGS index_granularity = 8192
"#;

/// SQL for creating the sessions table.
pub const CREATE_SESSIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id UUID,
    tenant_id UUID,
    user_id Nullable(String),
    started_at DateTime64(3),
    ended_at Nullable(DateTime64(3)),
    event_count UInt64,
    duration_ms Nullable(UInt64),

    -- First/last event info
    entry_path Nullable(String),
    exit_path Nullable(String),

    -- Aggregates
    pageview_count UInt32,
    click_count UInt32,
    scroll_max_depth Nullable(Float64),

    -- Partitioning
    session_date Date DEFAULT toDate(started_at)
)
ENGINE = MergeTree()
PARTITION BY (tenant_id, toYYYYMM(session_date))
ORDER BY (tenant_id, started_at)
TTL session_date + INTERVAL 90 DAY
SETTINGS index_granularity = 8192
"#;

/// SQL for creating the internal metrics table (dogfooding).
pub const CREATE_METRICS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS internal_metrics (
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

/// All table creation statements.
pub fn all_tables() -> Vec<&'static str> {
    vec![
        CREATE_EVENTS_TABLE,
        CREATE_SESSIONS_TABLE,
        CREATE_METRICS_TABLE,
    ]
}
