//! Consumer worker for reading events from Redpanda and inserting to ClickHouse.
//!
//! This worker implements the core data pipeline:
//! 1. Fetch batch of events from Redpanda
//! 2. Insert batch to ClickHouse
//! 3. Commit offset (at-least-once delivery)
//! 4. Repeat

use clickhouse_client::ClickHouseClient;
use engine_core::Result;
use redpanda::Consumer;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Consumer worker configuration.
#[derive(Debug, Clone)]
pub struct ConsumerWorkerConfig {
    /// Maximum retries for ClickHouse insert failures
    pub max_retries: u32,
    /// Backoff between retries
    pub retry_backoff: Duration,
    /// Whether to continue on insert failure (skip batch)
    pub skip_on_failure: bool,
}

impl Default for ConsumerWorkerConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retry_backoff: Duration::from_millis(100),
            skip_on_failure: true,
        }
    }
}

/// Worker that consumes events from Redpanda and inserts to ClickHouse.
pub struct ConsumerWorker {
    consumer: Arc<Consumer>,
    clickhouse: Arc<ClickHouseClient>,
    config: ConsumerWorkerConfig,
}

impl ConsumerWorker {
    /// Creates a new consumer worker.
    pub fn new(
        consumer: Arc<Consumer>,
        clickhouse: Arc<ClickHouseClient>,
    ) -> Self {
        Self {
            consumer,
            clickhouse,
            config: ConsumerWorkerConfig::default(),
        }
    }

    /// Creates a new consumer worker with custom config.
    pub fn with_config(
        consumer: Arc<Consumer>,
        clickhouse: Arc<ClickHouseClient>,
        config: ConsumerWorkerConfig,
    ) -> Self {
        Self {
            consumer,
            clickhouse,
            config,
        }
    }

    /// Main run loop - fetch, insert, commit.
    ///
    /// This runs indefinitely, processing batches of events.
    pub async fn run(&self) -> Result<()> {
        info!(
            topic = %self.consumer.config().topic,
            group_id = %self.consumer.config().group_id,
            batch_size = self.consumer.config().batch_size,
            "Consumer worker starting"
        );

        loop {
            match self.process_batch().await {
                Ok(count) => {
                    if count > 0 {
                        debug!(count = count, "Processed batch");
                    }
                }
                Err(e) => {
                    error!("Batch processing error: {}", e);
                    // Brief pause before retrying
                    tokio::time::sleep(Duration::from_secs(1)).await;

                    // Reset connection on error
                    self.consumer.reset_connection().await;
                }
            }
        }
    }

    /// Processes a single batch: fetch → insert → commit.
    async fn process_batch(&self) -> Result<usize> {
        // 1. Fetch batch from Redpanda
        let (events, offset) = self.consumer.fetch_batch().await?;

        if events.is_empty() {
            return Ok(0);
        }

        let count = events.len();

        // 2. Insert to ClickHouse with retries
        let insert_result = self.insert_with_retry(events).await;

        match insert_result {
            Ok(inserted) => {
                // 3. Commit offset after successful insert
                if let Some(offset) = offset {
                    self.consumer.commit(offset).await?;
                }

                Ok(inserted)
            }
            Err(e) => {
                error!(
                    count = count,
                    error = %e,
                    "Failed to insert batch after retries"
                );

                if self.config.skip_on_failure {
                    // Skip this batch and commit anyway to avoid infinite retry
                    warn!("Skipping failed batch, committing offset");
                    if let Some(offset) = offset {
                        self.consumer.commit(offset).await?;
                    }
                    Ok(0)
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Inserts events with retry logic.
    async fn insert_with_retry(
        &self,
        events: Vec<engine_core::ClickHouseEvent>,
    ) -> Result<usize> {
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                let backoff = self.config.retry_backoff * attempt;
                warn!(
                    attempt = attempt,
                    backoff_ms = %backoff.as_millis(),
                    "Retrying ClickHouse insert"
                );
                tokio::time::sleep(backoff).await;
            }

            match clickhouse_client::insert::insert_clickhouse_events(
                &self.clickhouse,
                events.clone(),
            )
            .await
            {
                Ok(count) => return Ok(count),
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            engine_core::Error::internal("Insert failed with unknown error")
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consumer_worker_config_defaults() {
        let config = ConsumerWorkerConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_backoff, Duration::from_millis(100));
        assert!(config.skip_on_failure);
    }
}
