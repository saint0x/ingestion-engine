//! Retention worker for TTL enforcement.

use clickhouse_client::ClickHouseClient;
use engine_core::RetentionTier;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Worker that enforces retention policies.
pub struct RetentionWorker {
    #[allow(dead_code)] // Will be used when retention queries are implemented
    clickhouse: Arc<ClickHouseClient>,
}

impl RetentionWorker {
    pub fn new(clickhouse: Arc<ClickHouseClient>) -> Self {
        Self { clickhouse }
    }

    /// Run retention enforcement.
    pub async fn run(&self) -> Result<(), String> {
        info!("Running retention worker");

        // Enforce retention for each tier
        for tier in [RetentionTier::Free, RetentionTier::Paid, RetentionTier::Enterprise] {
            if let Err(e) = self.enforce_tier_retention(tier).await {
                warn!(tier = ?tier, error = %e, "Failed to enforce retention");
            }
        }

        debug!("Retention check complete");
        Ok(())
    }

    async fn enforce_tier_retention(&self, tier: RetentionTier) -> Result<(), String> {
        let retention_hours = tier.raw_event_retention_hours();

        // In production, this would:
        // 1. Query for tenant_ids with this tier
        // 2. Delete events older than retention period
        // 3. Update metrics

        // ClickHouse TTL handles most of this automatically,
        // but we may need manual cleanup for tier changes

        debug!(
            tier = ?tier,
            retention_hours = retention_hours,
            "Checked retention for tier"
        );

        Ok(())
    }
}
