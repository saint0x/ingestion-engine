//! End-to-end tests for the ingest pipeline.
//!
//! These tests validate the full data flow using mock Kafka:
//! POST /overwatch-ingest → MockProducer (captures events) → ClickHouse
//!
//! The MockProducer implements the same EventProducer trait as the real
//! Producer, so we test all production code paths except the actual
//! Kafka network transport.
//!
//! Requires Docker to be running for ClickHouse testcontainer.

use axum_test::TestServer;
use clickhouse_client::{count_all_events, query_all_events, truncate_events};
use integration_tests::{fixtures, setup::TestContext};

/// Full pipeline test: POST /overwatch-ingest → MockProducer → ClickHouse (array format)
#[tokio::test]
async fn test_ingest_array_format_e2e() {
    // Setup test environment
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    // Ensure clean state
    truncate_events(&ctx.clickhouse).await.ok();
    ctx.clear_captured();

    // Send events via /overwatch-ingest (array format)
    let events = fixtures::sdk_events(5);
    let payload = fixtures::array_payload(events);

    let response = server
        .post("/overwatch-ingest")
        .content_type("application/json")
        .add_header("X-API-Key", &fixtures::test_api_key())
        .bytes(payload.into())
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["success"], true);
    assert_eq!(body["received"], 5);

    // Verify MockProducer captured the events
    let captured = ctx.captured_events();
    assert_eq!(
        captured.len(),
        5,
        "MockProducer should have captured 5 events"
    );
    assert!(
        captured.iter().all(|e| e.event_type == "pageview"),
        "All captured events should be pageview type"
    );

    // Process captured events to ClickHouse (simulates ConsumerWorker)
    let processed = ctx
        .process_captured_events()
        .await
        .expect("Failed to process events");
    assert_eq!(processed, 5, "Should have processed 5 events");

    // Verify data in ClickHouse
    let count = count_all_events(&ctx.clickhouse)
        .await
        .expect("Count query failed");
    assert_eq!(count, 5, "Expected 5 events in ClickHouse, got {}", count);

    let rows = query_all_events(&ctx.clickhouse, 10)
        .await
        .expect("Query failed");
    assert_eq!(rows.len(), 5, "Expected 5 rows, got {}", rows.len());
    assert!(
        rows.iter().all(|r| r.event_type == "pageview"),
        "All events should be pageview type"
    );

    // Verify project_id matches expected (from mock auth)
    let expected_project = fixtures::expected_project_id(&fixtures::test_api_key());
    assert!(
        rows.iter().all(|r| r.project_id == expected_project),
        "All events should have project_id {}",
        expected_project
    );
}

/// Full pipeline test: POST /overwatch-ingest → MockProducer → ClickHouse (object format)
#[tokio::test]
async fn test_ingest_object_format_e2e() {
    // Setup test environment
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    // Ensure clean state
    truncate_events(&ctx.clickhouse).await.ok();
    ctx.clear_captured();

    // Send events via /overwatch-ingest (object format with metadata)
    let events = fixtures::sdk_events_of_type(3, "click");
    let payload = fixtures::object_payload(events);

    let response = server
        .post("/overwatch-ingest")
        .content_type("application/json")
        .add_header("X-API-Key", &fixtures::test_api_key())
        .bytes(payload.into())
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["success"], true);
    assert_eq!(body["received"], 3);

    // Verify MockProducer captured the events
    let captured = ctx.captured_events();
    assert_eq!(
        captured.len(),
        3,
        "MockProducer should have captured 3 events"
    );
    assert!(
        captured.iter().all(|e| e.event_type == "click"),
        "All captured events should be click type"
    );

    // Process captured events to ClickHouse
    ctx.process_captured_events()
        .await
        .expect("Failed to process events");

    // Verify data in ClickHouse
    let count = count_all_events(&ctx.clickhouse)
        .await
        .expect("Count query failed");
    assert_eq!(count, 3, "Expected 3 events in ClickHouse, got {}", count);

    let rows = query_all_events(&ctx.clickhouse, 10)
        .await
        .expect("Query failed");
    assert!(
        rows.iter().all(|r| r.event_type == "click"),
        "All events should be click type"
    );
}

/// Full pipeline test: POST /overwatch-ingest → MockProducer → ClickHouse (single event)
#[tokio::test]
async fn test_ingest_single_event_e2e() {
    // Setup test environment
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    // Ensure clean state
    truncate_events(&ctx.clickhouse).await.ok();
    ctx.clear_captured();

    // Send single event via /overwatch-ingest
    let event = fixtures::sdk_event("scroll");
    let payload = fixtures::single_payload(event);

    let response = server
        .post("/overwatch-ingest")
        .content_type("application/json")
        .add_header("X-API-Key", &fixtures::test_api_key())
        .bytes(payload.into())
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["success"], true);
    assert_eq!(body["received"], 1);

    // Verify MockProducer captured the event
    let captured = ctx.captured_events();
    assert_eq!(
        captured.len(),
        1,
        "MockProducer should have captured 1 event"
    );
    assert_eq!(captured[0].event_type, "scroll");

    // Process captured events to ClickHouse
    ctx.process_captured_events()
        .await
        .expect("Failed to process events");

    // Verify data in ClickHouse
    let count = count_all_events(&ctx.clickhouse)
        .await
        .expect("Count query failed");
    assert_eq!(count, 1, "Expected 1 event in ClickHouse, got {}", count);

    let rows = query_all_events(&ctx.clickhouse, 10)
        .await
        .expect("Query failed");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].event_type, "scroll");
}

/// Test multiple event types in a single batch
#[tokio::test]
async fn test_ingest_mixed_event_types_e2e() {
    // Setup test environment
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    // Ensure clean state
    truncate_events(&ctx.clickhouse).await.ok();
    ctx.clear_captured();

    // Create batch with multiple event types
    let mut events = Vec::new();
    events.push(fixtures::sdk_event("pageview"));
    events.push(fixtures::sdk_event("click"));
    events.push(fixtures::sdk_event("scroll"));
    events.push(fixtures::sdk_event("performance"));
    events.push(fixtures::sdk_event("custom"));

    let payload = fixtures::array_payload(events);

    let response = server
        .post("/overwatch-ingest")
        .content_type("application/json")
        .add_header("X-API-Key", &fixtures::test_api_key())
        .bytes(payload.into())
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["received"], 5);

    // Verify MockProducer captured all events
    let captured = ctx.captured_events();
    assert_eq!(
        captured.len(),
        5,
        "MockProducer should have captured 5 events"
    );

    let captured_types: std::collections::HashSet<_> =
        captured.iter().map(|e| e.event_type.as_str()).collect();
    assert!(captured_types.contains("pageview"));
    assert!(captured_types.contains("click"));
    assert!(captured_types.contains("scroll"));
    assert!(captured_types.contains("performance"));
    assert!(captured_types.contains("custom"));

    // Process captured events to ClickHouse
    ctx.process_captured_events()
        .await
        .expect("Failed to process events");

    // Verify all event types present in ClickHouse
    let rows = query_all_events(&ctx.clickhouse, 10)
        .await
        .expect("Query failed");
    assert_eq!(rows.len(), 5);

    let event_types: std::collections::HashSet<_> =
        rows.iter().map(|r| r.event_type.as_str()).collect();
    assert!(event_types.contains("pageview"));
    assert!(event_types.contains("click"));
    assert!(event_types.contains("scroll"));
    assert!(event_types.contains("performance"));
    assert!(event_types.contains("custom"));
}

/// Test that producer failures are handled gracefully
#[tokio::test]
async fn test_producer_failure_returns_error() {
    // Setup test environment
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    // Set producer to fail mode
    ctx.set_producer_failure(true);

    // Send events via /overwatch-ingest
    let events = fixtures::sdk_events(2);
    let payload = fixtures::array_payload(events);

    let response = server
        .post("/overwatch-ingest")
        .content_type("application/json")
        .add_header("X-API-Key", &fixtures::test_api_key())
        .bytes(payload.into())
        .await;

    // Should get 500 error when producer fails
    response.assert_status(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
}
