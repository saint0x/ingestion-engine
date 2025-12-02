//! Application state shared across handlers.

use clickhouse_client::ClickHouseClient;
use redpanda::Producer;
use std::sync::Arc;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Redpanda producer
    pub producer: Arc<Producer>,
    /// ClickHouse client
    pub clickhouse: Arc<ClickHouseClient>,
}

impl AppState {
    pub fn new(producer: Arc<Producer>, clickhouse: Arc<ClickHouseClient>) -> Self {
        Self {
            producer,
            clickhouse,
        }
    }
}
