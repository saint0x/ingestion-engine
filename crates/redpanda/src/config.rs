//! Redpanda configuration.

use serde::{Deserialize, Serialize};

/// Redpanda producer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedpandaConfig {
    /// Broker addresses
    pub brokers: Vec<String>,
    /// Batch size (number of events)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Batch timeout in milliseconds
    #[serde(default = "default_batch_timeout_ms")]
    pub batch_timeout_ms: u64,
    /// Compression type (none, gzip, snappy, lz4, zstd)
    #[serde(default = "default_compression")]
    pub compression: String,
    /// Request timeout in milliseconds
    #[serde(default = "default_request_timeout_ms")]
    pub request_timeout_ms: u64,
    /// Number of retries
    #[serde(default = "default_retries")]
    pub retries: u32,
    /// Retry backoff in milliseconds
    #[serde(default = "default_retry_backoff_ms")]
    pub retry_backoff_ms: u64,
    /// Acks required (0, 1, -1/all)
    #[serde(default = "default_acks")]
    pub acks: String,
}

fn default_batch_size() -> usize {
    1000
}

fn default_batch_timeout_ms() -> u64 {
    100
}

fn default_compression() -> String {
    "lz4".to_string()
}

fn default_request_timeout_ms() -> u64 {
    30000
}

fn default_retries() -> u32 {
    3
}

fn default_retry_backoff_ms() -> u64 {
    100
}

fn default_acks() -> String {
    "all".to_string()
}

impl Default for RedpandaConfig {
    fn default() -> Self {
        Self {
            brokers: vec!["localhost:9092".to_string()],
            batch_size: default_batch_size(),
            batch_timeout_ms: default_batch_timeout_ms(),
            compression: default_compression(),
            request_timeout_ms: default_request_timeout_ms(),
            retries: default_retries(),
            retry_backoff_ms: default_retry_backoff_ms(),
            acks: default_acks(),
        }
    }
}

impl RedpandaConfig {
    /// Returns the broker list as a comma-separated string.
    pub fn broker_string(&self) -> String {
        self.brokers.join(",")
    }
}
