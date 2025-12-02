//! Tests for error handling in the ingest pipeline.
//!
//! These tests verify that the API returns correct error codes for various failure scenarios.
//! Uses MockProducer instead of real Kafka to avoid Docker Desktop AIO limits.
//!
//! Requires Docker to be running for ClickHouse testcontainer.

use axum::http::StatusCode;
use axum_test::TestServer;
use integration_tests::{fixtures, setup::TestContext};

/// Test missing API key returns AUTH_001
#[tokio::test]
async fn test_missing_api_key_returns_401() {
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    let events = fixtures::sdk_events(1);
    let payload = fixtures::array_payload(events);

    let response = server
        .post("/ingest")
        .content_type("application/json")
        // No API key header
        .bytes(payload.into())
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["code"], "AUTH_001",
        "Expected AUTH_001 for missing API key"
    );
}

/// Test invalid API key format returns AUTH_002
#[tokio::test]
async fn test_invalid_api_key_format_returns_401() {
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    let events = fixtures::sdk_events(1);
    let payload = fixtures::array_payload(events);

    let response = server
        .post("/ingest")
        .content_type("application/json")
        .add_header("X-API-Key", "invalid_key_format")
        .bytes(payload.into())
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["code"], "AUTH_002",
        "Expected AUTH_002 for invalid API key format"
    );
}

/// Test API key with wrong prefix returns AUTH_002
#[tokio::test]
async fn test_wrong_api_key_prefix_returns_401() {
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    let events = fixtures::sdk_events(1);
    let payload = fixtures::array_payload(events);

    // Wrong prefix - should be owk_test_ or owk_live_
    let response = server
        .post("/ingest")
        .content_type("application/json")
        .add_header("X-API-Key", "owk_prod_ABC123xyz789DEF456ghi012JKL345mn")
        .bytes(payload.into())
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["code"], "AUTH_002",
        "Expected AUTH_002 for wrong API key prefix"
    );
}

/// Test invalid JSON returns VALID_001
#[tokio::test]
async fn test_invalid_json_returns_400() {
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    let response = server
        .post("/ingest")
        .content_type("application/json")
        .add_header("X-API-Key", &fixtures::test_api_key())
        .bytes("not json at all".into())
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["code"], "VALID_001",
        "Expected VALID_001 for invalid JSON"
    );
}

/// Test malformed JSON (truncated) returns VALID_001
#[tokio::test]
async fn test_malformed_json_returns_400() {
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    let response = server
        .post("/ingest")
        .content_type("application/json")
        .add_header("X-API-Key", &fixtures::test_api_key())
        .bytes(r#"[{"id": "123", "type": "#.into())
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["code"], "VALID_001",
        "Expected VALID_001 for malformed JSON"
    );
}

/// Test batch exceeding 1000 events returns VALID_002
#[tokio::test]
async fn test_batch_exceeds_limit_returns_400() {
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    // Generate 1001 events (exceeds 1000 limit)
    let events = fixtures::oversized_batch();
    let payload = fixtures::array_payload(events);

    let response = server
        .post("/ingest")
        .content_type("application/json")
        .add_header("X-API-Key", &fixtures::test_api_key())
        .bytes(payload.into())
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["code"], "VALID_002",
        "Expected VALID_002 for batch exceeding limit"
    );
}

/// Test empty batch returns appropriate response
#[tokio::test]
async fn test_empty_batch_accepted() {
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    let response = server
        .post("/ingest")
        .content_type("application/json")
        .add_header("X-API-Key", &fixtures::test_api_key())
        .bytes("[]".into())
        .await;

    // Empty batch should be accepted (nothing to process)
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["success"], true);
    assert_eq!(body["received"], 0);
}

/// Test event missing required field returns VALID_001
#[tokio::test]
async fn test_event_missing_required_field_returns_400() {
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    // Event missing 'type' field
    let payload = r#"[{"id": "550e8400-e29b-41d4-a716-446655440000", "timestamp": 1704067200000, "sessionId": "11111111-1111-1111-1111-111111111111", "url": "https://example.com/", "userAgent": "Mozilla/5.0"}]"#;

    let response = server
        .post("/ingest")
        .content_type("application/json")
        .add_header("X-API-Key", &fixtures::test_api_key())
        .bytes(payload.into())
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["code"], "VALID_001",
        "Expected VALID_001 for missing required field"
    );
}

/// Test event with invalid event type returns VALID_001
#[tokio::test]
async fn test_invalid_event_type_returns_400() {
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    // Event with invalid type
    let payload = r#"[{"id": "550e8400-e29b-41d4-a716-446655440000", "type": "invalid_type", "timestamp": 1704067200000, "sessionId": "11111111-1111-1111-1111-111111111111", "url": "https://example.com/", "userAgent": "Mozilla/5.0"}]"#;

    let response = server
        .post("/ingest")
        .content_type("application/json")
        .add_header("X-API-Key", &fixtures::test_api_key())
        .bytes(payload.into())
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["code"], "VALID_001",
        "Expected VALID_001 for invalid event type"
    );
}

/// Test Authorization header with Bearer token works
#[tokio::test]
async fn test_bearer_token_auth_works() {
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    let events = fixtures::sdk_events(1);
    let payload = fixtures::array_payload(events);

    // Use Authorization: Bearer instead of X-API-Key
    let response = server
        .post("/ingest")
        .content_type("application/json")
        .add_header(
            "Authorization",
            &format!("Bearer {}", fixtures::test_api_key()),
        )
        .bytes(payload.into())
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["success"], true);
}

/// Test both live and test API keys are accepted
#[tokio::test]
async fn test_both_key_environments_work() {
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    // Test key
    let events = fixtures::sdk_events(1);
    let payload = fixtures::array_payload(events);

    let response = server
        .post("/ingest")
        .content_type("application/json")
        .add_header("X-API-Key", &fixtures::test_api_key())
        .bytes(payload.into())
        .await;
    response.assert_status_ok();

    // Live key
    let events = fixtures::sdk_events(1);
    let payload = fixtures::array_payload(events);

    let response = server
        .post("/ingest")
        .content_type("application/json")
        .add_header("X-API-Key", &fixtures::live_api_key())
        .bytes(payload.into())
        .await;
    response.assert_status_ok();
}
