//! Redpanda health checks.

use crate::config::RedpandaConfig;
use rskafka::client::{ClientBuilder, Credentials, SaslConfig};
use std::sync::Arc;
use tracing::{debug, error, info};

/// Creates a TLS configuration for Redpanda Cloud.
fn create_tls_config() -> Arc<rustls::ClientConfig> {
    let root_store =
        rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    Arc::new(config)
}

/// Check Redpanda connection health.
pub async fn check_connection(config: &RedpandaConfig) -> bool {
    let connection = config.broker_string();
    let mut builder = ClientBuilder::new(vec![connection.clone()]);

    info!(
        broker = %connection,
        has_username = config.sasl_username.is_some(),
        has_password = config.sasl_password.is_some(),
        "Checking Redpanda connection"
    );

    // Add TLS and SASL auth if credentials provided (for Redpanda Cloud)
    if let (Some(username), Some(password)) = (&config.sasl_username, &config.sasl_password) {
        info!("Enabling TLS and SASL/SCRAM-SHA-256 authentication for Redpanda Cloud");
        builder = builder
            .tls_config(create_tls_config())
            .sasl_config(SaslConfig::ScramSha256(Credentials::new(
                username.clone(),
                password.clone(),
            )));
    } else {
        info!("No SASL credentials provided, using plain connection");
    }

    match builder.build().await {
        Ok(client) => {
            // Try to list topics to verify connection
            match client.list_topics().await {
                Ok(topics) => {
                    debug!(topics = topics.len(), "Redpanda connection healthy");
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
    let mut builder = ClientBuilder::new(vec![connection]);

    // Add TLS and SASL auth if credentials provided (for Redpanda Cloud)
    if let (Some(username), Some(password)) = (&config.sasl_username, &config.sasl_password) {
        builder = builder
            .tls_config(create_tls_config())
            .sasl_config(SaslConfig::ScramSha256(Credentials::new(
                username.clone(),
                password.clone(),
            )));
    }

    match builder.build().await {
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
