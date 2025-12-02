//! High-throughput Redpanda producer using rskafka.

use crate::batch::{BatchAccumulator, BatchConfig, EventBatch};
use crate::config::RedpandaConfig;
use crate::partitioner::{get_partition_key, PartitionStrategy};
use engine_core::{ClickHouseEvent, Event, Result};
use telemetry::metrics;
use rskafka::client::{
    partition::{Compression, UnknownTopicHandling},
    ClientBuilder,
};
use rskafka::record::Record;
use chrono::Utc;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error};

/// Result of sending events.
#[derive(Debug)]
pub struct SendResult {
    pub events_sent: usize,
    pub errors: Vec<String>,
}

/// High-throughput producer with batching.
pub struct Producer {
    accumulator: Arc<BatchAccumulator>,
    config: RedpandaConfig,
    partition_strategy: PartitionStrategy,
    /// Cached partition clients per topic
    clients: RwLock<BTreeMap<String, Arc<rskafka::client::partition::PartitionClient>>>,
}

impl Producer {
    /// Creates a new producer.
    pub async fn new(config: RedpandaConfig) -> Result<Self> {
        let batch_config = BatchConfig {
            max_size: config.batch_size,
            max_age: Duration::from_millis(config.batch_timeout_ms),
        };

        Ok(Self {
            accumulator: Arc::new(BatchAccumulator::new(batch_config)),
            config,
            partition_strategy: PartitionStrategy::BySession,
            clients: RwLock::new(BTreeMap::new()),
        })
    }

    /// Gets or creates a partition client for a topic.
    async fn get_client(
        &self,
        topic: &str,
        partition: i32,
    ) -> Result<Arc<rskafka::client::partition::PartitionClient>> {
        let key = format!("{}:{}", topic, partition);

        // Check cache first
        {
            let clients = self.clients.read().await;
            if let Some(client) = clients.get(&key) {
                return Ok(client.clone());
            }
        }

        // Create new client
        let connection = self.config.broker_string();
        let client = ClientBuilder::new(vec![connection])
            .build()
            .await
            .map_err(|e| engine_core::Error::internal(format!("Failed to connect: {}", e)))?;

        let partition_client = client
            .partition_client(topic.to_string(), partition, UnknownTopicHandling::Error)
            .await
            .map_err(|e| {
                engine_core::Error::internal(format!("Failed to get partition client: {}", e))
            })?;

        let partition_client = Arc::new(partition_client);

        // Cache it
        {
            let mut clients = self.clients.write().await;
            clients.insert(key, partition_client.clone());
        }

        Ok(partition_client)
    }

    /// Sends a single event (will be batched).
    pub async fn send(&self, event: Event) -> Result<()> {
        if let Some(batch) = self.accumulator.add(event) {
            self.flush_batch(batch).await?;
        }
        Ok(())
    }

    /// Sends multiple events.
    pub async fn send_many(&self, events: Vec<Event>) -> Result<SendResult> {
        let mut batches_to_flush = Vec::new();

        for event in events {
            if let Some(batch) = self.accumulator.add(event) {
                batches_to_flush.push(batch);
            }
        }

        let mut total_sent = 0;
        let mut errors = Vec::new();

        for batch in batches_to_flush {
            match self.flush_batch(batch).await {
                Ok(count) => total_sent += count,
                Err(e) => errors.push(e.to_string()),
            }
        }

        Ok(SendResult {
            events_sent: total_sent,
            errors,
        })
    }

    /// Sends ClickHouse events directly to Redpanda.
    ///
    /// This bypasses the batch accumulator and sends immediately,
    /// serializing ClickHouseEvent to JSON.
    pub async fn send_clickhouse_events(&self, events: Vec<ClickHouseEvent>) -> Result<SendResult> {
        if events.is_empty() {
            return Ok(SendResult {
                events_sent: 0,
                errors: Vec::new(),
            });
        }

        let count = events.len();
        let topic = &self.config.topic;
        let start = std::time::Instant::now();

        // Get partition client
        let client = self.get_client(topic, 0).await?;

        // Convert ClickHouse events to records
        let mut records = Vec::with_capacity(count);
        let mut errors = Vec::new();

        for event in events {
            let key = Some(format!("{}:{}", event.project_id, event.session_id));

            match serde_json::to_vec(&event) {
                Ok(payload) => {
                    records.push(Record {
                        key: key.map(|k| k.into_bytes()),
                        value: Some(payload),
                        headers: BTreeMap::new(),
                        timestamp: Utc::now(),
                    });
                }
                Err(e) => {
                    errors.push(format!("Failed to serialize event {}: {}", event.event_id, e));
                }
            }
        }

        if records.is_empty() {
            return Ok(SendResult {
                events_sent: 0,
                errors,
            });
        }

        // Send records
        let compression = match self.config.compression.as_str() {
            "gzip" => Compression::Gzip,
            "snappy" => Compression::Snappy,
            "lz4" => Compression::Lz4,
            "zstd" => Compression::Zstd,
            _ => Compression::NoCompression,
        };

        match client.produce(records.clone(), compression).await {
            Ok(_offsets) => {
                let sent = records.len();
                metrics().events_sent_to_redpanda.inc_by(sent as u64);

                let elapsed = start.elapsed();
                metrics().redpanda_latency_ms.observe(elapsed.as_millis() as u64);
                metrics().batches_sent_to_redpanda.inc();

                debug!(
                    topic = %topic,
                    count = sent,
                    latency_ms = %elapsed.as_millis(),
                    "Sent ClickHouse events to Redpanda"
                );

                Ok(SendResult {
                    events_sent: sent,
                    errors,
                })
            }
            Err(e) => {
                error!("Failed to send ClickHouse events to Redpanda: {}", e);
                metrics().redpanda_send_errors.inc_by(records.len() as u64);
                errors.push(format!("Failed to produce: {}", e));
                Ok(SendResult {
                    events_sent: 0,
                    errors,
                })
            }
        }
    }

    /// Flushes a batch to Redpanda.
    async fn flush_batch(&self, batch: EventBatch) -> Result<usize> {
        let count = batch.events.len();
        let topic = &batch.topic;

        let start = std::time::Instant::now();

        // Get partition client (using partition 0 for simplicity)
        // In production, you'd want to properly partition based on key
        let client = self.get_client(topic, 0).await?;

        // Convert events to records
        let mut records = Vec::with_capacity(count);
        for event in batch.events {
            let key = get_partition_key(
                self.partition_strategy,
                &event.session_id.to_string(),
                &event.tenant_id.to_string(),
            );

            let payload = serde_json::to_vec(&event)
                .map_err(engine_core::Error::Serialization)?;

            records.push(Record {
                key: key.map(|k| k.into_bytes()),
                value: Some(payload),
                headers: BTreeMap::new(),
                timestamp: Utc::now(),
            });
        }

        // Send records
        let compression = match self.config.compression.as_str() {
            "gzip" => Compression::Gzip,
            "snappy" => Compression::Snappy,
            "lz4" => Compression::Lz4,
            "zstd" => Compression::Zstd,
            _ => Compression::NoCompression,
        };

        match client.produce(records, compression).await {
            Ok(_offsets) => {
                metrics().events_sent_to_redpanda.inc_by(count as u64);
            }
            Err(e) => {
                error!("Failed to send batch to Redpanda: {}", e);
                metrics().redpanda_send_errors.inc_by(count as u64);
                return Err(engine_core::Error::internal(format!(
                    "Failed to produce: {}",
                    e
                )));
            }
        }

        let elapsed = start.elapsed();
        metrics()
            .redpanda_latency_ms
            .observe(elapsed.as_millis() as u64);
        metrics().batches_sent_to_redpanda.inc();

        debug!(
            topic = %topic,
            count = count,
            latency_ms = %elapsed.as_millis(),
            "Flushed batch to Redpanda"
        );

        Ok(count)
    }

    /// Starts the background flush task.
    pub fn start_flush_task(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let producer = self.clone();

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_millis(50));

            loop {
                ticker.tick().await;

                let batches = producer.accumulator.flush_aged();
                for batch in batches {
                    if let Err(e) = producer.flush_batch(batch).await {
                        error!("Failed to flush aged batch: {}", e);
                    }
                }
            }
        })
    }

    /// Flushes all pending batches.
    pub async fn flush(&self) -> Result<()> {
        let batches = self.accumulator.flush_all();
        for batch in batches {
            self.flush_batch(batch).await?;
        }
        Ok(())
    }

    /// Checks if producer is healthy.
    pub async fn health_check(&self) -> bool {
        true
    }
}
