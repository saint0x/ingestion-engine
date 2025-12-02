//! Backfill worker for metric recomputation.

use clickhouse_client::ClickHouseClient;
use std::sync::Arc;
use tracing::{debug, info};

/// Worker that recomputes derived metrics.
pub struct BackfillWorker {
    #[allow(dead_code)] // Will be used when backfill queries are implemented
    clickhouse: Arc<ClickHouseClient>,
}

impl BackfillWorker {
    pub fn new(clickhouse: Arc<ClickHouseClient>) -> Self {
        Self { clickhouse }
    }

    /// Run backfill for a specific tenant and date range.
    pub async fn run(
        &self,
        tenant_id: &str,
        start_date: chrono::NaiveDate,
        end_date: chrono::NaiveDate,
    ) -> Result<BackfillResult, String> {
        info!(
            tenant_id = tenant_id,
            start = %start_date,
            end = %end_date,
            "Running backfill"
        );

        // In production, this would:
        // 1. Recompute session aggregates
        // 2. Recompute daily/hourly metrics
        // 3. Update materialized views

        let result = BackfillResult {
            tenant_id: tenant_id.to_string(),
            events_processed: 0,
            sessions_updated: 0,
            metrics_recomputed: 0,
        };

        debug!(result = ?result, "Backfill complete");
        Ok(result)
    }

    /// Recompute session data.
    pub async fn recompute_sessions(&self, _tenant_id: &str) -> Result<u64, String> {
        // Would run session aggregation query against self.clickhouse
        Ok(0)
    }
}

/// Result of backfill operation.
#[derive(Debug)]
pub struct BackfillResult {
    pub tenant_id: String,
    pub events_processed: u64,
    pub sessions_updated: u64,
    pub metrics_recomputed: u64,
}
