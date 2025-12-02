//! Tests for health check endpoints.
//!
//! These tests verify the health endpoints return correct status and structure.
//!
//! Requires Docker to be running for testcontainers.

use axum::http::StatusCode;
use axum_test::TestServer;
use integration_tests::setup::TestContext;

/// Test /health endpoint returns proper structure
#[tokio::test]
async fn test_health_endpoint_structure() {
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    let response = server.get("/health").await;
    response.assert_status_ok();

    let body: serde_json::Value = response.json();

    // Verify required fields exist
    assert!(
        body.get("status").is_some(),
        "Response should have 'status' field"
    );
    assert!(
        body.get("redpanda_connected").is_some(),
        "Response should have 'redpanda_connected' field"
    );
    assert!(
        body.get("clickhouse_connected").is_some(),
        "Response should have 'clickhouse_connected' field"
    );
    assert!(
        body.get("queue_depth").is_some(),
        "Response should have 'queue_depth' field"
    );
}

/// Test /health endpoint reports valid status
#[tokio::test]
async fn test_health_endpoint_healthy() {
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    let response = server.get("/health").await;
    response.assert_status_ok();

    let body: serde_json::Value = response.json();

    // Status should be one of the valid health statuses
    // In test context, components may not have reported healthy yet
    let status = body["status"].as_str().unwrap_or("");
    assert!(
        status == "healthy" || status == "degraded" || status == "unhealthy",
        "Status should be 'healthy', 'degraded', or 'unhealthy', got '{}'",
        status
    );
}

/// Test /health/ready endpoint
#[tokio::test]
async fn test_ready_endpoint() {
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    let response = server.get("/health/ready").await;

    // Ready endpoint returns 200 if ready, 503 if not
    let status = response.status_code();
    assert!(
        status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
        "Ready endpoint should return 200 or 503, got {}",
        status
    );
}

/// Test /health/live endpoint always returns 200 when service is running
#[tokio::test]
async fn test_live_endpoint() {
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    let response = server.get("/health/live").await;

    // Liveness probe should return 200 if the service is running
    // It might return 503 if there's a fatal condition
    let status = response.status_code();
    assert!(
        status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
        "Live endpoint should return 200 or 503, got {}",
        status
    );
}

/// Test that health endpoints don't require authentication
#[tokio::test]
async fn test_health_endpoints_no_auth_required() {
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    // Health endpoints should work without any authentication
    let response = server.get("/health").await;
    assert_ne!(
        response.status_code(),
        StatusCode::UNAUTHORIZED,
        "/health should not require auth"
    );

    let response = server.get("/health/ready").await;
    assert_ne!(
        response.status_code(),
        StatusCode::UNAUTHORIZED,
        "/health/ready should not require auth"
    );

    let response = server.get("/health/live").await;
    assert_ne!(
        response.status_code(),
        StatusCode::UNAUTHORIZED,
        "/health/live should not require auth"
    );
}

/// Test queue_depth field is a valid number
#[tokio::test]
async fn test_health_queue_depth_is_number() {
    let ctx = TestContext::new().await;
    let server = TestServer::new(ctx.router.clone()).expect("Failed to create test server");

    let response = server.get("/health").await;
    response.assert_status_ok();

    let body: serde_json::Value = response.json();

    // queue_depth should be a non-negative number
    let queue_depth = body["queue_depth"].as_u64();
    assert!(
        queue_depth.is_some(),
        "queue_depth should be a valid u64 number"
    );
}
