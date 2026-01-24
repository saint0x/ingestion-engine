//! ClickHouse operational metrics for self-hosted monitoring.
//!
//! Collects metrics from ClickHouse system tables to monitor:
//! - Parts count and merge pressure
//! - Active merges
//! - Disk usage

use crate::client::ClickHouseClient;
use clickhouse::Row;
use engine_core::Result;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, warn};

/// Parts count per table from system.parts.
#[derive(Debug, Clone, Row, Deserialize, Serialize)]
pub struct TablePartsInfo {
    pub database: String,
    pub table: String,
    #[serde(rename = "parts_count")]
    pub parts_count: u64,
    #[serde(rename = "total_rows")]
    pub rows: u64,
    #[serde(rename = "total_bytes")]
    pub bytes_on_disk: u64,
    #[serde(rename = "active_parts")]
    pub active_parts: u64,
}

/// Active merge info from system.merges.
#[derive(Debug, Clone, Row, Deserialize, Serialize)]
pub struct MergeInfo {
    pub database: String,
    pub table: String,
    pub elapsed: f64,
    pub progress: f64,
    pub num_parts: u64,
    pub total_size_bytes_compressed: u64,
}

/// Disk usage info from system.disks.
#[derive(Debug, Clone, Row, Deserialize, Serialize)]
pub struct DiskInfo {
    pub name: String,
    pub path: String,
    pub free_space: u64,
    pub total_space: u64,
}

/// ClickHouse operational metrics snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickHouseOpsMetrics {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub tables: Vec<TablePartsInfo>,
    pub active_merges: Vec<MergeInfo>,
    pub disks: Vec<DiskInfo>,
    pub merge_pressure_score: f64,
    pub max_parts_count: u64,
    pub total_active_merges: usize,
}

impl ClickHouseOpsMetrics {
    /// Check if merge pressure is critical (should trigger alert).
    pub fn is_merge_pressure_critical(&self) -> bool {
        self.merge_pressure_score > 70.0
    }

    /// Check if merge pressure is elevated (should trigger warning).
    pub fn is_merge_pressure_elevated(&self) -> bool {
        self.merge_pressure_score > 40.0
    }

    /// Get tables with high parts count (> 300).
    pub fn high_parts_tables(&self) -> Vec<&TablePartsInfo> {
        self.tables.iter().filter(|t| t.active_parts > 300).collect()
    }

    /// Get disks with high usage (> 85%).
    pub fn high_usage_disks(&self) -> Vec<(&DiskInfo, f64)> {
        self.disks
            .iter()
            .filter_map(|d| {
                if d.total_space > 0 {
                    let usage_pct = 100.0 - (d.free_space as f64 / d.total_space as f64 * 100.0);
                    if usage_pct > 85.0 {
                        return Some((d, usage_pct));
                    }
                }
                None
            })
            .collect()
    }
}

/// Collect ClickHouse operational metrics.
pub async fn collect_ops_metrics(client: &ClickHouseClient) -> Result<ClickHouseOpsMetrics> {
    let tables = get_table_parts_info(client).await.unwrap_or_else(|e| {
        error!(error = %e, "Failed to get table parts info");
        vec![]
    });

    let active_merges = get_active_merges(client).await.unwrap_or_else(|e| {
        debug!(error = %e, "Failed to get active merges (may be normal if no merges running)");
        vec![]
    });

    let disks = get_disk_info(client).await.unwrap_or_else(|e| {
        error!(error = %e, "Failed to get disk info");
        vec![]
    });

    // Calculate merge pressure score
    let merge_pressure_score = calculate_merge_pressure(&tables, &active_merges);
    let max_parts_count = tables.iter().map(|t| t.active_parts).max().unwrap_or(0);

    Ok(ClickHouseOpsMetrics {
        timestamp: chrono::Utc::now(),
        tables,
        total_active_merges: active_merges.len(),
        active_merges,
        disks,
        merge_pressure_score,
        max_parts_count,
    })
}

/// Get parts count per table in overwatch database.
async fn get_table_parts_info(client: &ClickHouseClient) -> Result<Vec<TablePartsInfo>> {
    let sql = r#"
        SELECT
            database,
            table,
            count() as parts_count,
            sum(rows) as total_rows,
            sum(bytes_on_disk) as total_bytes,
            countIf(active) as active_parts
        FROM system.parts
        WHERE database = 'overwatch'
        GROUP BY database, table
        ORDER BY parts_count DESC
    "#;

    let rows: Vec<TablePartsInfo> = client
        .inner()
        .query(sql)
        .fetch_all()
        .await
        .map_err(|e| engine_core::Error::internal(format!("Query error: {}", e)))?;

    Ok(rows)
}

/// Get currently running merges.
async fn get_active_merges(client: &ClickHouseClient) -> Result<Vec<MergeInfo>> {
    let sql = r#"
        SELECT
            database,
            table,
            elapsed,
            progress,
            num_parts,
            total_size_bytes_compressed
        FROM system.merges
        WHERE database = 'overwatch'
        ORDER BY elapsed DESC
    "#;

    let rows: Vec<MergeInfo> = client
        .inner()
        .query(sql)
        .fetch_all()
        .await
        .map_err(|e| engine_core::Error::internal(format!("Query error: {}", e)))?;

    Ok(rows)
}

/// Get disk usage information.
async fn get_disk_info(client: &ClickHouseClient) -> Result<Vec<DiskInfo>> {
    let sql = r#"
        SELECT
            name,
            path,
            free_space,
            total_space
        FROM system.disks
    "#;

    let rows: Vec<DiskInfo> = client
        .inner()
        .query(sql)
        .fetch_all()
        .await
        .map_err(|e| engine_core::Error::internal(format!("Query error: {}", e)))?;

    Ok(rows)
}

/// Calculate a merge pressure score (0-100).
///
/// Higher values indicate more merge pressure and potential issues.
/// Factors considered:
/// - Parts count per table (>300 warning, >1000 critical)
/// - Number of active merges (>5 warning, >10 critical)
/// - Duration of longest merge (>60s warning, >300s critical)
fn calculate_merge_pressure(tables: &[TablePartsInfo], merges: &[MergeInfo]) -> f64 {
    // Parts count factor: 0-50 points
    // >300 parts starts adding pressure, >1000 is critical
    let max_parts = tables.iter().map(|t| t.active_parts).max().unwrap_or(0) as f64;
    let parts_score = if max_parts > 1000.0 {
        50.0
    } else if max_parts > 300.0 {
        ((max_parts - 300.0) / 700.0 * 50.0).min(50.0)
    } else {
        0.0
    };

    // Active merge count factor: 0-25 points
    let merge_count = merges.len() as f64;
    let merge_count_score = if merge_count > 10.0 {
        25.0
    } else if merge_count > 5.0 {
        ((merge_count - 5.0) / 5.0 * 25.0).min(25.0)
    } else {
        0.0
    };

    // Merge duration factor: 0-25 points
    // Long-running merges indicate the system is struggling
    let max_merge_time = merges.iter().map(|m| m.elapsed).fold(0.0, f64::max);
    let merge_time_score = if max_merge_time > 300.0 {
        25.0
    } else if max_merge_time > 60.0 {
        ((max_merge_time - 60.0) / 240.0 * 25.0).min(25.0)
    } else {
        0.0
    };

    parts_score + merge_count_score + merge_time_score
}

/// Log operational metrics with appropriate severity levels.
pub fn log_ops_metrics(metrics: &ClickHouseOpsMetrics) {
    // Log merge pressure
    if metrics.is_merge_pressure_critical() {
        error!(
            merge_pressure = metrics.merge_pressure_score,
            max_parts = metrics.max_parts_count,
            active_merges = metrics.total_active_merges,
            "CRITICAL: ClickHouse merge pressure is critical"
        );
    } else if metrics.is_merge_pressure_elevated() {
        warn!(
            merge_pressure = metrics.merge_pressure_score,
            max_parts = metrics.max_parts_count,
            active_merges = metrics.total_active_merges,
            "ClickHouse merge pressure elevated"
        );
    } else {
        debug!(
            merge_pressure = metrics.merge_pressure_score,
            max_parts = metrics.max_parts_count,
            active_merges = metrics.total_active_merges,
            "ClickHouse operational metrics collected"
        );
    }

    // Log high parts tables
    for table in metrics.high_parts_tables() {
        warn!(
            table = format!("{}.{}", table.database, table.table),
            parts = table.active_parts,
            rows = table.rows,
            "Table has high parts count - consider increasing batch size"
        );
    }

    // Log disk usage warnings
    for (disk, usage_pct) in metrics.high_usage_disks() {
        warn!(
            disk = disk.name,
            path = disk.path,
            usage_percent = format!("{:.1}%", usage_pct),
            free_gb = disk.free_space / 1_000_000_000,
            "Disk usage high"
        );
    }
}
