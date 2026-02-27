//! Common test setup functions.

use api::{router, state::AppState};
use axum::Router;
use clickhouse_client::{
    insert::insert_clickhouse_events, schema::init_schema, ClickHouseClient, ClickHouseConfig,
};
use engine_core::{ClickHouseEvent, Result};
use redpanda::EventProducer;
use std::sync::Arc;

use crate::containers::TestContainers;
use crate::mocks::MockProducer;

/// Test context with mock Kafka and real ClickHouse.
///
/// This provides the same production code paths by:
/// - Using the real Axum router with all middleware
/// - Using MockProducer which implements EventProducer trait
/// - Using real ClickHouse testcontainer for storage verification
pub struct TestContext {
    pub containers: TestContainers,
    pub clickhouse: Arc<ClickHouseClient>,
    pub mock_producer: Arc<MockProducer>,
    pub router: Router,
}

impl TestContext {
    /// Create a new test context with all components initialized.
    pub async fn new() -> Self {
        // Start ClickHouse container (Redpanda is mocked)
        let containers = TestContainers::start().await;

        // Create ClickHouse client
        let ch_config = ClickHouseConfig {
            url: containers.clickhouse_url.clone(),
            database: containers.clickhouse_database.clone(),
            username: containers.clickhouse_username.clone(),
            password: containers.clickhouse_password.clone(),
            pool_size: 5,
            timeout_secs: 30,
        };
        let clickhouse =
            Arc::new(ClickHouseClient::new(ch_config).expect("Failed to create ClickHouse client"));

        // Initialize schema
        init_schema(&clickhouse)
            .await
            .expect("Failed to initialize schema");

        // Use mock producer instead of real Redpanda
        let mock_producer = Arc::new(MockProducer::new());

        // Create router with mock producer
        let state = AppState::new(
            mock_producer.clone() as Arc<dyn EventProducer>,
            clickhouse.clone(),
            "mock",
        );
        let router = router(state);

        Self {
            containers,
            clickhouse,
            mock_producer,
            router,
        }
    }

    /// Get all events captured by the mock producer.
    pub fn captured_events(&self) -> Vec<ClickHouseEvent> {
        self.mock_producer.captured_events()
    }

    /// Get count of captured events.
    pub fn captured_event_count(&self) -> usize {
        self.mock_producer.event_count()
    }

    /// Clear captured events (use between tests).
    pub fn clear_captured(&self) {
        self.mock_producer.clear();
    }

    /// Process captured events by inserting them to ClickHouse.
    ///
    /// This simulates what the ConsumerWorker does in production:
    /// it takes events from Kafka and inserts them to ClickHouse.
    pub async fn process_captured_events(&self) -> Result<usize> {
        let events = self.captured_events();
        if events.is_empty() {
            return Ok(0);
        }

        insert_clickhouse_events(&self.clickhouse, events).await
    }

    /// Get the ClickHouse URL.
    pub fn clickhouse_url(&self) -> &str {
        &self.containers.clickhouse_url
    }

    /// Set the mock producer to fail (for error testing).
    pub fn set_producer_failure(&self, should_fail: bool) {
        self.mock_producer.set_should_fail(should_fail);
    }
}
