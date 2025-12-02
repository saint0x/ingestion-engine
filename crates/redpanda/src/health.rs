//! Redpanda health checks.

use crate::config::RedpandaConfig;
use rskafka::client::ClientBuilder;
use std::time::Duration;
use tracing::{debug, error};

/// Check Redpanda connection health.
pub async fn check_connection(config: &RedpandaConfig) -> bool {
    let connection = config.broker_string();

    match ClientBuilder::new(vec![connection])
        .build()
        .await
    {
        Ok(client) => {
            // Try to list topics to verify connection
            match client.list_topics().await {
                Ok(topics) => {
                    debug!(
                        topics = topics.len(),
                        "Redpanda connection healthy"
                    );
                    true
                }
                Err(e) => {
                    error!("Failed to list Redpanda topics: {}", e);
                    false
                }
            }
        }
        Err(e) => {
            error!("Failed to connect to Redpanda: {}", e);
            false
        }
    }
}

/// Verify required topics exist.
pub async fn verify_topics(config: &RedpandaConfig, topics: &[&str]) -> Vec<String> {
    let connection = config.broker_string();

    match ClientBuilder::new(vec![connection]).build().await {
        Ok(client) => match client.list_topics().await {
            Ok(existing_topics) => {
                let existing: std::collections::HashSet<_> =
                    existing_topics.iter().map(|t| t.name.as_str()).collect();

                topics
                    .iter()
                    .filter(|t| !existing.contains(*t))
                    .map(|t| t.to_string())
                    .collect()
            }
            Err(_) => topics.iter().map(|t| t.to_string()).collect(),
        },
        Err(_) => topics.iter().map(|t| t.to_string()).collect(),
    }
}
