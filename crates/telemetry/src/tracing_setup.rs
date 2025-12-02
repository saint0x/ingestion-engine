//! Tracing setup for structured logging.

use tracing::Level;
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

/// Tracing configuration.
pub struct TracingConfig {
    /// Log level filter (e.g., "info", "debug", "ingestion_engine=debug")
    pub filter: String,
    /// Whether to output JSON format
    pub json: bool,
    /// Whether to include span events
    pub span_events: bool,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            filter: "info".to_string(),
            json: false,
            span_events: false,
        }
    }
}

impl TracingConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = filter.into();
        self
    }

    pub fn with_json(mut self, json: bool) -> Self {
        self.json = json;
        self
    }

    pub fn with_span_events(mut self, span_events: bool) -> Self {
        self.span_events = span_events;
        self
    }
}

/// Initialize tracing with the given configuration.
pub fn init_tracing(config: TracingConfig) {
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(&config.filter))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let span_events = if config.span_events {
        FmtSpan::NEW | FmtSpan::CLOSE
    } else {
        FmtSpan::NONE
    };

    if config.json {
        let fmt_layer = fmt::layer()
            .json()
            .with_span_events(span_events)
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
    } else {
        let fmt_layer = fmt::layer()
            .with_span_events(span_events)
            .with_target(true)
            .with_thread_ids(false)
            .with_file(false)
            .with_line_number(false);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
    }

    tracing::info!("Tracing initialized with filter: {}", config.filter);
}

/// Initialize tracing from environment variables.
pub fn init_tracing_from_env() {
    let json = std::env::var("LOG_JSON")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());

    init_tracing(TracingConfig::new().with_filter(filter).with_json(json));
}
