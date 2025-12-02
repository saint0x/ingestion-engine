//! Compression worker for free tier data rollup.

use clickhouse_client::ClickHouseClient;
use engine_core::RetentionTier;
use std::sync::Arc;
use tracing::{debug, info};

/// Worker that compresses old data for free tier tenants.
pub struct CompressionWorker {
    #[allow(dead_code)] // Will be used when compression queries are implemented
    clickhouse: Arc<ClickHouseClient>,
}

impl CompressionWorker {
    pub fn new(clickhouse: Arc<ClickHouseClient>) -> Self {
        Self { clickhouse }
    }

    /// Run compression for eligible data.
    pub async fn run(&self) -> Result<(), String> {
        info!("Running compression worker");

        // Find tenants with free tier that have data older than compression threshold
        let threshold_hours = RetentionTier::Free.compression_after_hours();

        // In production, this would:
        // 1. Query for tenant_ids with free tier
        // 2. For each tenant, find partitions older than threshold
        // 3. Compress raw events into aggregated form
        // 4. Delete the raw events

        // Placeholder query - in production would use proper aggregation
        let _compress_query = format!(
            r#"
            -- Compression placeholder
            -- Would aggregate events older than {} hours for free tier
            SELECT 1
            "#,
            threshold_hours
        );

        debug!(
            threshold_hours = threshold_hours,
            "Compression check complete"
        );

        Ok(())
    }
}

/// Aggregated event data for compression.
#[derive(Debug)]
pub struct AggregatedEvents {
    pub tenant_id: String,
    pub date: chrono::NaiveDate,
    pub event_type: String,
    pub count: u64,
    pub unique_sessions: u64,
    pub unique_users: u64,
}
