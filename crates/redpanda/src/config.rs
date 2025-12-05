//! Redpanda configuration.

use serde::{Deserialize, Deserializer, Serialize};

/// Deserialize brokers as either a comma-separated string or a list.
fn deserialize_brokers<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct BrokersVisitor;

    impl<'de> Visitor<'de> for BrokersVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a comma-separated string or a list of broker addresses")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(value.split(',').map(|s| s.trim().to_string()).collect())
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut brokers = Vec::new();
            while let Some(broker) = seq.next_element::<String>()? {
                brokers.push(broker);
            }
            Ok(brokers)
        }
    }

    deserializer.deserialize_any(BrokersVisitor)
}

/// Consumer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerConfig {
    /// Consumer group ID
    #[serde(default = "default_group_id")]
    pub group_id: String,
    /// Topic to consume from
    #[serde(default = "default_topic")]
    pub topic: String,
    /// Batch size (number of events before processing)
    #[serde(default = "default_consumer_batch_size")]
    pub batch_size: usize,
    /// Batch timeout in milliseconds
    #[serde(default = "default_consumer_batch_timeout_ms")]
    pub batch_timeout_ms: u64,
    /// Session timeout in milliseconds
    #[serde(default = "default_session_timeout_ms")]
    pub session_timeout_ms: u64,
    /// Whether to auto-commit offsets (false = manual commit)
    #[serde(default)]
    pub auto_commit: bool,
}

fn default_group_id() -> String {
    "ingestion-engine".to_string()
}

fn default_consumer_batch_size() -> usize {
    1000
}

fn default_consumer_batch_timeout_ms() -> u64 {
    1000 // 1 second
}

fn default_session_timeout_ms() -> u64 {
    30000 // 30 seconds
}

impl Default for ConsumerConfig {
    fn default() -> Self {
        Self {
            group_id: default_group_id(),
            topic: default_topic(),
            batch_size: default_consumer_batch_size(),
            batch_timeout_ms: default_consumer_batch_timeout_ms(),
            session_timeout_ms: default_session_timeout_ms(),
            auto_commit: false,
        }
    }
}

/// Redpanda producer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedpandaConfig {
    /// Broker addresses (comma-separated string or list)
    #[serde(deserialize_with = "deserialize_brokers", default = "default_brokers")]
    pub brokers: Vec<String>,
    /// SASL username (for cloud authentication)
    pub sasl_username: Option<String>,
    /// SASL password (for cloud authentication)
    pub sasl_password: Option<String>,
    /// Default topic for processed events
    #[serde(default = "default_topic")]
    pub topic: String,
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
    /// Consumer configuration
    #[serde(default)]
    pub consumer: ConsumerConfig,
}

fn default_brokers() -> Vec<String> {
    vec!["localhost:9092".to_string()]
}

fn default_topic() -> String {
    "events".to_string()
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
            brokers: default_brokers(),
            sasl_username: None,
            sasl_password: None,
            topic: default_topic(),
            batch_size: default_batch_size(),
            batch_timeout_ms: default_batch_timeout_ms(),
            compression: default_compression(),
            request_timeout_ms: default_request_timeout_ms(),
            retries: default_retries(),
            retry_backoff_ms: default_retry_backoff_ms(),
            acks: default_acks(),
            consumer: ConsumerConfig::default(),
        }
    }
}

impl RedpandaConfig {
    /// Returns the broker list as a comma-separated string.
    pub fn broker_string(&self) -> String {
        self.brokers.join(",")
    }
}
