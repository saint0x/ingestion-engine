//! ClickHouse client wrapper.

use crate::config::ClickHouseConfig;
use clickhouse::Client;
use engine_core::Result;
use std::sync::Arc;
use tracing::info;

/// ClickHouse client wrapper with connection pooling.
#[derive(Clone)]
pub struct ClickHouseClient {
    inner: Client,
    config: ClickHouseConfig,
}

impl ClickHouseClient {
    /// Creates a new ClickHouse client.
    pub fn new(config: ClickHouseConfig) -> Result<Self> {
        let mut client = Client::default()
            .with_url(&config.url)
            .with_database(&config.database);

        if let Some(ref user) = config.username {
            client = client.with_user(user);
        }

        if let Some(ref pass) = config.password {
            client = client.with_password(pass);
        }

        info!(
            url = %config.url,
            database = %config.database,
            "Created ClickHouse client"
        );

        Ok(Self {
            inner: client,
            config,
        })
    }

    /// Returns the inner clickhouse client.
    pub fn inner(&self) -> &Client {
        &self.inner
    }

    /// Returns the configuration.
    pub fn config(&self) -> &ClickHouseConfig {
        &self.config
    }
}
