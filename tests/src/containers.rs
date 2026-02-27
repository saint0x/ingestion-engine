//! Testcontainer setup for ClickHouse.
//!
//! Note: Redpanda is mocked instead of using a real container because
//! Docker Desktop on macOS has AIO resource limits that cause Redpanda
//! to hang during startup.

use std::time::Duration;
use testcontainers::{
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
    ContainerAsync, GenericImage, ImageExt,
};

/// Container handle for ClickHouse.
pub struct TestContainers {
    #[allow(dead_code)]
    clickhouse: Option<ContainerAsync<GenericImage>>,
    pub clickhouse_url: String,
    pub clickhouse_database: String,
    pub clickhouse_username: Option<String>,
    pub clickhouse_password: Option<String>,
}

impl TestContainers {
    /// Start ClickHouse container.
    pub async fn start() -> Self {
        if let Some(url) = std::env::var("OVERWATCH_TEST_CLICKHOUSE_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())
        {
            return Self {
                clickhouse: None,
                clickhouse_url: url,
                clickhouse_database: std::env::var("OVERWATCH_TEST_CLICKHOUSE_DB")
                    .unwrap_or_else(|_| "overwatch".to_string()),
                clickhouse_username: std::env::var("OVERWATCH_TEST_CLICKHOUSE_USER").ok(),
                clickhouse_password: std::env::var("OVERWATCH_TEST_CLICKHOUSE_PASSWORD").ok(),
            };
        }

        let (clickhouse, clickhouse_url) = start_clickhouse().await;

        Self {
            clickhouse: Some(clickhouse),
            clickhouse_url,
            clickhouse_database: "overwatch".to_string(),
            clickhouse_username: Some("default".to_string()),
            clickhouse_password: None,
        }
    }
}

/// Start ClickHouse container, return container and HTTP URL.
pub async fn start_clickhouse() -> (ContainerAsync<GenericImage>, String) {
    // Use duration-based wait, then verify HTTP endpoint
    // CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT=1 allows creating users without password
    let image = GenericImage::new("clickhouse/clickhouse-server", "24.3")
        .with_wait_for(WaitFor::seconds(5))
        .with_exposed_port(8123.tcp())
        .with_exposed_port(9000.tcp())
        .with_env_var("CLICKHOUSE_DB", "overwatch")
        .with_env_var("CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT", "1")
        .with_env_var("CLICKHOUSE_USER", "default")
        .with_env_var("CLICKHOUSE_PASSWORD", "");

    let container = image.start().await.expect("Failed to start ClickHouse");

    let port = container.get_host_port_ipv4(8123).await.unwrap();
    let url = format!("http://127.0.0.1:{}", port);

    // Wait for HTTP endpoint to be ready
    wait_for_http(&url, Duration::from_secs(30)).await;

    (container, url)
}

/// Wait for HTTP endpoint to respond.
async fn wait_for_http(url: &str, timeout: Duration) {
    let client = reqwest::Client::new();
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        if let Ok(resp) = client.get(url).send().await {
            if resp.status().is_success() {
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    panic!("HTTP endpoint {} not ready after {:?}", url, timeout);
}
