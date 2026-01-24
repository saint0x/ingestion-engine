//! Retention worker for partition-based data deletion.
//!
//! Instead of row-level TTL (which causes continuous background mutations),
//! this worker drops entire partitions that are older than the retention period.

use chrono::{Datelike, Utc};
use clickhouse::Row;
use clickhouse_client::ClickHouseClient;
use serde::Deserialize;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// All tables that need retention enforcement via partition drops.
/// These are partitioned by `toYYYYMM(timestamp)` or `toYYYYMM(started_at)`.
const RETENTION_TABLES: &[&str] = &[
    "overwatch.events",
    "overwatch.sessions",
    "overwatch.pageviews",
    "overwatch.clicks",
    "overwatch.scroll_events",
    "overwatch.mouse_moves",
    "overwatch.form_events",
    "overwatch.errors",
    "overwatch.performance_metrics",
    "overwatch.visibility_events",
    "overwatch.resource_loads",
    "overwatch.geographic",
    "overwatch.custom_events",
];

/// Internal metrics table has shorter retention (30 days).
const METRICS_TABLE: &str = "overwatch.internal_metrics";

/// Default retention in months for data tables.
const DEFAULT_RETENTION_MONTHS: u32 = 3; // ~90 days

/// Retention in months for internal metrics.
const METRICS_RETENTION_MONTHS: u32 = 1; // ~30 days

/// Partition info from system.parts.
#[derive(Debug, Clone, Row, Deserialize)]
struct PartitionInfo {
    partition: String,
    partition_id: String,
    #[serde(rename = "total_rows")]
    rows: u64,
    #[serde(rename = "total_bytes")]
    bytes_on_disk: u64,
}

/// Worker that enforces retention policies by dropping old partitions.
pub struct RetentionWorker {
    clickhouse: Arc<ClickHouseClient>,
}

impl RetentionWorker {
    pub fn new(clickhouse: Arc<ClickHouseClient>) -> Self {
        Self { clickhouse }
    }

    /// Run retention enforcement across all tables.
    pub async fn run(&self) -> Result<(), String> {
        info!("Running retention worker - partition-based deletion");

        let now = Utc::now();

        // Enforce retention for data tables (90 days = ~3 months)
        let data_cutoff = calculate_cutoff_partition(now, DEFAULT_RETENTION_MONTHS);
        info!(
            cutoff_partition = %data_cutoff,
            retention_months = DEFAULT_RETENTION_MONTHS,
            "Enforcing data table retention"
        );

        for table in RETENTION_TABLES {
            if let Err(e) = self.drop_old_partitions(table, &data_cutoff).await {
                warn!(table = table, error = %e, "Failed to enforce retention");
            }
        }

        // Enforce retention for internal metrics (30 days = ~1 month)
        let metrics_cutoff = calculate_cutoff_partition(now, METRICS_RETENTION_MONTHS);
        info!(
            cutoff_partition = %metrics_cutoff,
            retention_months = METRICS_RETENTION_MONTHS,
            "Enforcing metrics table retention"
        );

        if let Err(e) = self.drop_old_partitions(METRICS_TABLE, &metrics_cutoff).await {
            warn!(table = METRICS_TABLE, error = %e, "Failed to enforce metrics retention");
        }

        info!("Retention check complete");
        Ok(())
    }

    /// Drop partitions older than the cutoff.
    async fn drop_old_partitions(&self, table: &str, cutoff_partition: &str) -> Result<(), String> {
        // Query for partitions older than cutoff
        let partitions = self.get_old_partitions(table, cutoff_partition).await?;

        if partitions.is_empty() {
            debug!(table = table, cutoff = cutoff_partition, "No partitions to drop");
            return Ok(());
        }

        let mut dropped_count = 0;
        let mut dropped_rows = 0u64;
        let mut dropped_bytes = 0u64;

        for partition in &partitions {
            info!(
                table = table,
                partition = %partition.partition,
                partition_id = %partition.partition_id,
                rows = partition.rows,
                bytes = partition.bytes_on_disk,
                "Dropping partition"
            );

            // Use partition_id (which is the numeric value like "202301")
            let sql = format!(
                "ALTER TABLE {} DROP PARTITION '{}'",
                table, partition.partition_id
            );

            match self.clickhouse.inner().query(&sql).execute().await {
                Ok(_) => {
                    dropped_count += 1;
                    dropped_rows += partition.rows;
                    dropped_bytes += partition.bytes_on_disk;
                }
                Err(e) => {
                    error!(
                        table = table,
                        partition = %partition.partition_id,
                        error = %e,
                        "Failed to drop partition"
                    );
                }
            }
        }

        if dropped_count > 0 {
            info!(
                table = table,
                dropped_partitions = dropped_count,
                dropped_rows = dropped_rows,
                dropped_bytes = dropped_bytes,
                dropped_bytes_human = %format_bytes(dropped_bytes),
                "Partition cleanup complete"
            );
        }

        Ok(())
    }

    /// Get partitions older than the cutoff from system.parts.
    async fn get_old_partitions(
        &self,
        table: &str,
        cutoff_partition: &str,
    ) -> Result<Vec<PartitionInfo>, String> {
        let (database, table_name) = table
            .split_once('.')
            .ok_or_else(|| format!("Invalid table name: {}", table))?;

        // Query system.parts for active partitions older than cutoff
        // Group by partition to get totals across all parts
        let sql = format!(
            r#"
            SELECT
                partition,
                partition_id,
                sum(rows) as total_rows,
                sum(bytes_on_disk) as total_bytes
            FROM system.parts
            WHERE database = '{}'
              AND table = '{}'
              AND active = 1
              AND partition_id < '{}'
            GROUP BY partition, partition_id
            ORDER BY partition_id
            "#,
            database, table_name, cutoff_partition
        );

        let rows: Vec<PartitionInfo> = self
            .clickhouse
            .inner()
            .query(&sql)
            .fetch_all()
            .await
            .map_err(|e| format!("Query error: {}", e))?;

        Ok(rows)
    }
}

/// Calculate the cutoff partition (YYYYMM format) for retention.
///
/// Partitions with ID less than this value should be dropped.
fn calculate_cutoff_partition(now: chrono::DateTime<Utc>, months_to_keep: u32) -> String {
    // Subtract months from current date
    let year = now.year();
    let month = now.month();

    // Calculate target year/month
    let total_months = year * 12 + month as i32 - months_to_keep as i32;
    let target_year = (total_months - 1) / 12;
    let target_month = ((total_months - 1) % 12) + 1;

    format!("{}{:02}", target_year, target_month)
}

/// Format bytes into human-readable string.
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_calculate_cutoff_partition() {
        // January 2024, keeping 3 months -> cutoff should be October 2023
        let now = Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap();
        assert_eq!(calculate_cutoff_partition(now, 3), "202310");

        // March 2024, keeping 3 months -> cutoff should be December 2023
        let now = Utc.with_ymd_and_hms(2024, 3, 15, 0, 0, 0).unwrap();
        assert_eq!(calculate_cutoff_partition(now, 3), "202312");

        // January 2024, keeping 1 month -> cutoff should be December 2023
        let now = Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap();
        assert_eq!(calculate_cutoff_partition(now, 1), "202312");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_bytes(1536 * 1024 * 1024), "1.50 GB");
    }
}
