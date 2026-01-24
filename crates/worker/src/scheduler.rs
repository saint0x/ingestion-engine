//! Worker scheduler for background tasks.

use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{error, info};

use clickhouse_client::ClickHouseClient;
use redpanda::Consumer;

use crate::compression::CompressionWorker;
use crate::consumer::ConsumerWorker;
use crate::notifications::NotificationWorker;
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
    /// Notification check interval
    pub notification_check_interval: Duration,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            compression_interval: Duration::from_secs(3600), // 1 hour
            retention_interval: Duration::from_secs(3600),   // 1 hour
            metrics_flush_interval: Duration::from_secs(60), // 1 minute
            notification_check_interval: Duration::from_secs(60), // 1 minute
        }
    }
}

/// Background worker scheduler.
pub struct WorkerScheduler {
    config: WorkerConfig,
    clickhouse: Arc<ClickHouseClient>,
    consumer: Option<Arc<Consumer>>,
}

impl WorkerScheduler {
    pub fn new(config: WorkerConfig, clickhouse: Arc<ClickHouseClient>) -> Self {
        Self {
            config,
            clickhouse,
            consumer: None,
        }
    }

    /// Creates a new scheduler with a consumer for the Redpanda → ClickHouse pipeline.
    pub fn with_consumer(
        config: WorkerConfig,
        clickhouse: Arc<ClickHouseClient>,
        consumer: Arc<Consumer>,
    ) -> Self {
        Self {
            config,
            clickhouse,
            consumer: Some(consumer),
        }
    }

    /// Starts all background workers.
    pub fn start(self: Arc<Self>) -> Vec<tokio::task::JoinHandle<()>> {
        let mut handles = Vec::new();

        // Consumer worker (Redpanda → ClickHouse)
        if let Some(ref consumer) = self.consumer {
            let consumer = consumer.clone();
            let clickhouse = self.clickhouse.clone();
            handles.push(tokio::spawn(async move {
                let worker = ConsumerWorker::new(consumer, clickhouse);
                if let Err(e) = worker.run().await {
                    error!("Consumer worker fatal error: {}", e);
                }
            }));
            info!("Consumer worker started");
        }

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

        // Notification worker
        let scheduler = self.clone();
        handles.push(tokio::spawn(async move {
            scheduler.run_notification_worker().await;
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
        use clickhouse_client::{collect_ops_metrics, log_ops_metrics};
        use telemetry::metrics;

        let mut ticker = interval(self.config.metrics_flush_interval);

        loop {
            ticker.tick().await;

            // Flush application metrics
            let snapshot = metrics().snapshot();
            if let Err(e) = insert_metrics(&self.clickhouse, snapshot).await {
                error!("Failed to flush metrics: {}", e);
            }

            // Collect and log ClickHouse operational metrics
            match collect_ops_metrics(&self.clickhouse).await {
                Ok(ops_metrics) => {
                    log_ops_metrics(&ops_metrics);
                }
                Err(e) => {
                    error!("Failed to collect ClickHouse ops metrics: {}", e);
                }
            }
        }
    }

    async fn run_notification_worker(&self) {
        use clickhouse_client::collect_ops_metrics;

        let worker = NotificationWorker::from_env();
        let mut ticker = interval(self.config.notification_check_interval);

        loop {
            ticker.tick().await;

            // Check application metrics for alerts
            if let Err(e) = worker.check_and_alert().await {
                error!("Notification check error: {}", e);
            }

            // Check ClickHouse ops metrics for alerts
            match collect_ops_metrics(&self.clickhouse).await {
                Ok(ops_metrics) => {
                    if let Err(e) = worker.check_clickhouse_ops(&ops_metrics).await {
                        error!("ClickHouse ops notification error: {}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to collect ops metrics for notifications: {}", e);
                }
            }
        }
    }
}
