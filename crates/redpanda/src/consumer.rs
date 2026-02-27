//! Redpanda consumer for reading events and inserting to ClickHouse.
//!
//! Uses rskafka for Kafka-compatible message consumption with:
//! - Manual offset management for at-least-once delivery
//! - Batch fetching with configurable size and timeout
//! - JSON deserialization of ClickHouseEvent records

use crate::config::ConsumerConfig;
use engine_core::{ClickHouseEvent, Result};
use rskafka::client::{
    partition::{OffsetAt, UnknownTopicHandling},
    ClientBuilder, Credentials, SaslConfig,
};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use telemetry::metrics;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Creates a TLS configuration for Redpanda Cloud.
fn create_tls_config() -> Arc<rustls::ClientConfig> {
    let root_store =
        rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    Arc::new(config)
}

/// Offset tracking for manual commit.
#[derive(Debug, Clone, Copy)]
pub struct Offset {
    pub partition: i32,
    pub offset: i64,
}

/// Consumer for reading events from Redpanda.
pub struct Consumer {
    config: ConsumerConfig,
    brokers: Vec<String>,
    /// SASL username (for cloud authentication)
    sasl_username: Option<String>,
    /// SASL password (for cloud authentication)
    sasl_password: Option<String>,
    /// Partition client (currently only partition 0)
    partition_client: RwLock<Option<Arc<rskafka::client::partition::PartitionClient>>>,
    /// Current offset (next offset to read)
    current_offset: AtomicI64,
    /// Whether consumer has been initialized
    initialized: std::sync::atomic::AtomicBool,
}

impl Consumer {
    /// Creates a new consumer.
    pub async fn new(
        config: ConsumerConfig,
        brokers: Vec<String>,
        sasl_username: Option<String>,
        sasl_password: Option<String>,
    ) -> Result<Self> {
        info!(
            group_id = %config.group_id,
            topic = %config.topic,
            batch_size = config.batch_size,
            "Creating Redpanda consumer"
        );

        Ok(Self {
            config,
            brokers,
            sasl_username,
            sasl_password,
            partition_client: RwLock::new(None),
            current_offset: AtomicI64::new(-1),
            initialized: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Initializes the consumer connection.
    async fn ensure_connected(&self) -> Result<Arc<rskafka::client::partition::PartitionClient>> {
        // Check if already connected
        {
            let client = self.partition_client.read().await;
            if let Some(ref c) = *client {
                return Ok(c.clone());
            }
        }

        // Create new connection
        let connection = self.brokers.join(",");
        let mut builder = ClientBuilder::new(vec![connection]);

        // Add TLS and SASL auth if credentials provided (for Redpanda Cloud)
        if let (Some(username), Some(password)) = (&self.sasl_username, &self.sasl_password) {
            builder = builder
                .tls_config(create_tls_config())
                .sasl_config(SaslConfig::ScramSha256(Credentials::new(
                    username.clone(),
                    password.clone(),
                )));
        }

        let client = builder.build().await.map_err(|e| {
            engine_core::Error::internal(format!("Failed to connect to Redpanda: {}", e))
        })?;

        let partition_client = client
            .partition_client(
                self.config.topic.clone(),
                0, // Partition 0 for now
                UnknownTopicHandling::Error,
            )
            .await
            .map_err(|e| {
                engine_core::Error::internal(format!("Failed to get partition client: {}", e))
            })?;

        let partition_client = Arc::new(partition_client);

        // Initialize offset if needed
        if !self.initialized.load(Ordering::SeqCst) {
            // Start from the beginning or latest based on config
            // For at-least-once, we typically start from latest on first run
            let offset = partition_client
                .get_offset(OffsetAt::Latest)
                .await
                .map_err(|e| {
                    engine_core::Error::internal(format!("Failed to get offset: {}", e))
                })?;

            self.current_offset.store(offset, Ordering::SeqCst);
            self.initialized.store(true, Ordering::SeqCst);

            info!(
                topic = %self.config.topic,
                partition = 0,
                offset = offset,
                "Consumer initialized at offset"
            );
        }

        // Cache client
        {
            let mut client_guard = self.partition_client.write().await;
            *client_guard = Some(partition_client.clone());
        }

        Ok(partition_client)
    }

    /// Fetches a batch of events from Redpanda.
    ///
    /// Blocks until batch_size events are available or batch_timeout expires.
    /// Returns the events and the offset to commit after processing.
    pub async fn fetch_batch(&self) -> Result<(Vec<ClickHouseEvent>, Option<Offset>)> {
        let client = self.ensure_connected().await?;

        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(self.config.batch_timeout_ms);
        let max_bytes = self.config.batch_size * 64 * 1024; // Assume ~64KB max per event

        let current = self.current_offset.load(Ordering::SeqCst);

        // Fetch records
        let (records, _watermark) = client
            .fetch_records(current, 1..max_bytes as i32, timeout.as_millis() as i32)
            .await
            .map_err(|e| {
                // Connection error - clear cached client
                error!("Fetch error: {}", e);
                engine_core::Error::internal(format!("Failed to fetch records: {}", e))
            })?;

        if records.is_empty() {
            return Ok((Vec::new(), None));
        }

        // Deserialize records
        let mut events = Vec::with_capacity(records.len());
        let mut errors = 0;
        let mut max_offset = current;

        for record in records {
            max_offset = record.offset.max(max_offset);

            if let Some(value) = record.record.value {
                match serde_json::from_slice::<ClickHouseEvent>(&value) {
                    Ok(event) => events.push(event),
                    Err(e) => {
                        errors += 1;
                        warn!(
                            offset = record.offset,
                            error = %e,
                            "Failed to deserialize event"
                        );
                    }
                }
            }
        }

        // Update metrics
        metrics().events_consumed.inc_by(events.len() as u64);
        if errors > 0 {
            metrics().consumer_errors.inc_by(errors);
        }

        let elapsed = start.elapsed();
        debug!(
            events = events.len(),
            errors = errors,
            offset_start = current,
            offset_end = max_offset,
            latency_ms = %elapsed.as_millis(),
            "Fetched batch from Redpanda"
        );

        // Return offset to commit (next offset after the last record)
        let commit_offset = if !events.is_empty() || max_offset > current {
            Some(Offset {
                partition: 0,
                offset: max_offset + 1,
            })
        } else {
            None
        };

        Ok((events, commit_offset))
    }

    /// Commits an offset after successful processing.
    ///
    /// This updates the internal offset tracker. For true consumer groups,
    /// this would commit to Kafka's __consumer_offsets topic.
    pub async fn commit(&self, offset: Offset) -> Result<()> {
        // Update internal offset tracker
        let prev = self.current_offset.swap(offset.offset, Ordering::SeqCst);

        debug!(
            partition = offset.partition,
            prev_offset = prev,
            new_offset = offset.offset,
            "Committed offset"
        );

        // In a full implementation, we would commit to Kafka here using
        // the ControllerClient's commit_offset API

        Ok(())
    }

    /// Returns the current consumer offset.
    pub fn current_offset(&self) -> i64 {
        self.current_offset.load(Ordering::SeqCst)
    }

    /// Returns the consumer configuration.
    pub fn config(&self) -> &ConsumerConfig {
        &self.config
    }

    /// Checks if the consumer is healthy.
    pub async fn health_check(&self) -> bool {
        match self.ensure_connected().await {
            Ok(_) => true,
            Err(e) => {
                error!("Consumer health check failed: {}", e);
                false
            }
        }
    }

    /// Resets the connection (for error recovery).
    pub async fn reset_connection(&self) {
        let mut client = self.partition_client.write().await;
        *client = None;
        info!("Consumer connection reset");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consumer_config_defaults() {
        let config = ConsumerConfig::default();
        assert_eq!(config.group_id, "ingestion-engine");
        assert_eq!(config.topic, "events");
        assert_eq!(config.batch_size, 5000);
        assert_eq!(config.batch_timeout_ms, 1000);
        assert!(!config.auto_commit);
    }
}
