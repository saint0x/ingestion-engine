//! ClickHouse configuration.

use serde::{Deserialize, Serialize};

/// ClickHouse client configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickHouseConfig {
    /// ClickHouse HTTP URL
    pub url: String,
    /// Database name
    #[serde(default = "default_database")]
    pub database: String,
    /// Username (optional)
    pub username: Option<String>,
    /// Password (optional)
    pub password: Option<String>,
    /// Connection pool size
    #[serde(default = "default_pool_size")]
    pub pool_size: usize,
    /// Query timeout in seconds
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_database() -> String {
    "overwatch".to_string()
}

fn default_pool_size() -> usize {
    10
}

fn default_timeout_secs() -> u64 {
    30
}

impl Default for ClickHouseConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:8123".to_string(),
            database: default_database(),
            username: None,
            password: None,
            pool_size: default_pool_size(),
            timeout_secs: default_timeout_secs(),
        }
    }
}
