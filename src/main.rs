//! Overwatch Analytics Ingestion Engine
//!
//! High-throughput event ingestion pipeline handling:
//! - SDK event validation and schema enforcement
//! - Redpanda batched publishing with ordering guarantees
//! - ClickHouse materialized view integration
//! - Background workers for compression, retention, and enrichment

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::signal;
use tracing::{error, info};

use api::{router, AppState};
use clickhouse_client::{ClickHouseClient, ClickHouseConfig};
use redpanda::{Consumer, Producer, RedpandaConfig};
use telemetry::{health, init_tracing_from_env};
use worker::{WorkerConfig, WorkerScheduler};

/// Application configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Config {
    #[serde(default = "default_host")]
    host: String,
    #[serde(default = "default_port")]
    port: u16,

    /// Auth service URL for API key validation
    #[serde(default = "default_auth_url")]
    auth_url: String,

    #[serde(default)]
    redpanda: RedpandaConfig,

    #[serde(default)]
    clickhouse: ClickHouseConfig,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_auth_url() -> String {
    "http://auth-service:8080".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            auth_url: default_auth_url(),
            redpanda: RedpandaConfig::default(),
            clickhouse: ClickHouseConfig::default(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Install rustls crypto provider BEFORE any TLS operations
    // rustls 0.23+ requires explicit crypto provider selection
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // Load .env file if present
    dotenvy::dotenv().ok();

    // Initialize tracing
    init_tracing_from_env();

    info!("Starting Overwatch Ingestion Engine v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config = load_config()?;

    // Debug: log config to verify environment variables are being read
    info!(
        brokers = ?config.redpanda.brokers,
        sasl_username = config.redpanda.sasl_username.as_deref().unwrap_or("none"),
        "Loaded Redpanda config"
    );

    // Initialize Redpanda producer
    let producer = Arc::new(
        Producer::new(config.redpanda.clone())
            .await
            .context("Failed to create Redpanda producer")?,
    );

    // Start producer flush task
    let producer_clone = producer.clone();
    let _flush_handle = producer_clone.start_flush_task();

    // Initialize ClickHouse client
    let clickhouse = Arc::new(
        ClickHouseClient::new(config.clickhouse.clone())
            .context("Failed to create ClickHouse client")?,
    );

    // Initialize ClickHouse schema
    if let Err(e) = clickhouse_client::health::init_schema(&clickhouse).await {
        error!("Failed to initialize ClickHouse schema: {}", e);
        // Continue anyway - schema might already exist
    }

    // Check health and update status
    check_health(&config, &clickhouse).await;

    // Initialize Redpanda consumer for the pipeline
    let consumer = Arc::new(
        Consumer::new(
            config.redpanda.consumer.clone(),
            config.redpanda.brokers.clone(),
            config.redpanda.sasl_username.clone(),
            config.redpanda.sasl_password.clone(),
        )
        .await
        .context("Failed to create Redpanda consumer")?,
    );

    // Start background workers with consumer
    let worker_scheduler = Arc::new(WorkerScheduler::with_consumer(
        WorkerConfig::default(),
        clickhouse.clone(),
        consumer.clone(),
    ));
    let _worker_handles = worker_scheduler.start();

    // Create application state
    let state = AppState::new(producer.clone(), clickhouse.clone(), &config.auth_url);

    // Start rate limiter cleanup background task
    let _rate_limiter_cleanup = state.start_rate_limiter_cleanup();
    info!("Started rate limiter cleanup task (every 5 minutes)");

    // Create router
    let app = router(state);

    // Start HTTP server
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .context("Invalid server address")?;

    info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("Failed to bind to address")?;

    // Run server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Server error")?;

    // Cleanup
    info!("Shutting down...");

    // Flush remaining events
    if let Err(e) = producer.flush().await {
        error!("Failed to flush producer: {}", e);
    }

    info!("Shutdown complete");
    Ok(())
}

/// Load configuration from files and environment.
fn load_config() -> Result<Config> {
    let config = config::Config::builder()
        // Start with defaults
        .add_source(config::Config::try_from(&Config::default())?)
        // Load from config file if exists
        .add_source(
            config::File::with_name("config/default")
                .required(false)
                .format(config::FileFormat::Toml),
        )
        // Override with environment variables
        .add_source(
            config::Environment::default()
                .separator("__")
                .prefix("INGESTION")
                .try_parsing(true),
        )
        .build()
        .context("Failed to build configuration")?;

    let mut config: Config = config
        .try_deserialize()
        .context("Failed to deserialize configuration")?;

    // Manual overrides for nested Redpanda config from environment
    // The config crate's nested parsing doesn't work reliably with underscored field names
    if let Ok(brokers) = std::env::var("INGESTION_REDPANDA_BROKERS") {
        config.redpanda.brokers = brokers.split(',').map(|s| s.trim().to_string()).collect();
    }
    if let Ok(username) = std::env::var("INGESTION_REDPANDA_SASL_USERNAME") {
        config.redpanda.sasl_username = Some(username);
    }
    if let Ok(password) = std::env::var("INGESTION_REDPANDA_SASL_PASSWORD") {
        config.redpanda.sasl_password = Some(password);
    }
    if let Ok(topic) = std::env::var("INGESTION_REDPANDA_TOPIC") {
        config.redpanda.topic = topic;
    }

    // Manual overrides for nested ClickHouse config
    if let Ok(url) = std::env::var("INGESTION_CLICKHOUSE_URL") {
        config.clickhouse.url = url;
    }
    if let Ok(database) = std::env::var("INGESTION_CLICKHOUSE_DATABASE") {
        config.clickhouse.database = database;
    }
    if let Ok(username) = std::env::var("INGESTION_CLICKHOUSE_USERNAME") {
        config.clickhouse.username = Some(username);
    }
    if let Ok(password) = std::env::var("INGESTION_CLICKHOUSE_PASSWORD") {
        config.clickhouse.password = Some(password);
    }

    // Auth URL override
    if let Ok(auth_url) = std::env::var("INGESTION_AUTH_URL") {
        config.auth_url = auth_url;
    }

    Ok(config)
}

/// Check component health on startup.
async fn check_health(config: &Config, clickhouse: &ClickHouseClient) {
    // Check Redpanda
    let redpanda_healthy = redpanda::health::check_connection(&config.redpanda).await;
    if redpanda_healthy {
        health().redpanda.set_healthy();
        info!("Redpanda connection: healthy");
    } else {
        health().redpanda.set_unhealthy("Connection failed");
        error!("Redpanda connection: unhealthy");
    }

    // Check ClickHouse
    let ch_healthy = clickhouse_client::health::check_connection(clickhouse).await;
    if ch_healthy {
        health().clickhouse.set_healthy();
        info!("ClickHouse connection: healthy");
    } else {
        health().clickhouse.set_unhealthy("Connection failed");
        error!("ClickHouse connection: unhealthy");
    }
}

/// Graceful shutdown signal handler.
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C signal");
        }
        _ = terminate => {
            info!("Received terminate signal");
        }
    }
}
