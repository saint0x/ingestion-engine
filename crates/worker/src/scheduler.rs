//! Worker scheduler for background tasks.

use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{error, info};

use clickhouse_client::ClickHouseClient;

use crate::compression::CompressionWorker;
use crate::retention::RetentionWorker;

/// Worker scheduler configuration.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Compression check interval
    pub compression_interval: Duration,
    /// Retention check interval
    pub retention_interval: Duration,
    /// Metrics flush interval
    pub metrics_flush_interval: Duration,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            compression_interval: Duration::from_secs(3600), // 1 hour
            retention_interval: Duration::from_secs(3600),   // 1 hour
            metrics_flush_interval: Duration::from_secs(60), // 1 minute
        }
    }
}

/// Background worker scheduler.
pub struct WorkerScheduler {
    config: WorkerConfig,
    clickhouse: Arc<ClickHouseClient>,
}

impl WorkerScheduler {
    pub fn new(config: WorkerConfig, clickhouse: Arc<ClickHouseClient>) -> Self {
        Self { config, clickhouse }
    }

    /// Starts all background workers.
    pub fn start(self: Arc<Self>) -> Vec<tokio::task::JoinHandle<()>> {
        let mut handles = Vec::new();

        // Compression worker
        let scheduler = self.clone();
        handles.push(tokio::spawn(async move {
            scheduler.run_compression_worker().await;
        }));

        // Retention worker
        let scheduler = self.clone();
        handles.push(tokio::spawn(async move {
            scheduler.run_retention_worker().await;
        }));

        // Metrics flush worker
        let scheduler = self.clone();
        handles.push(tokio::spawn(async move {
            scheduler.run_metrics_flush().await;
        }));

        info!("Background workers started");
        handles
    }

    async fn run_compression_worker(&self) {
        let worker = CompressionWorker::new(self.clickhouse.clone());
        let mut ticker = interval(self.config.compression_interval);

        loop {
            ticker.tick().await;

            if let Err(e) = worker.run().await {
                error!("Compression worker error: {}", e);
            }
        }
    }

    async fn run_retention_worker(&self) {
        let worker = RetentionWorker::new(self.clickhouse.clone());
        let mut ticker = interval(self.config.retention_interval);

        loop {
            ticker.tick().await;

            if let Err(e) = worker.run().await {
                error!("Retention worker error: {}", e);
            }
        }
    }

    async fn run_metrics_flush(&self) {
        use clickhouse_client::insert::insert_metrics;
        use telemetry::metrics;

        let mut ticker = interval(self.config.metrics_flush_interval);

        loop {
            ticker.tick().await;

            let snapshot = metrics().snapshot();
            if let Err(e) = insert_metrics(&self.clickhouse, snapshot).await {
                error!("Failed to flush metrics: {}", e);
            }
        }
    }
}
