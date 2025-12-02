//! Health check endpoints.

use axum::{extract::State, http::StatusCode, Json};
use telemetry::{health, metrics};

use crate::response::HealthResponse;
use crate::state::AppState;

/// GET /health - Full health check.
pub async fn health_handler(State(state): State<AppState>) -> Json<HealthResponse> {
    let report = health().report();

    Json(HealthResponse {
        status: format!("{:?}", report.status).to_lowercase(),
        redpanda_connected: health().redpanda.is_healthy(),
        clickhouse_connected: health().clickhouse.is_healthy(),
        queue_depth: metrics().queue_depth.get(),
    })
}

/// GET /health/ready - Readiness probe (can accept traffic).
pub async fn ready_handler() -> StatusCode {
    if health().is_ready() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

/// GET /health/live - Liveness probe (service is running).
pub async fn live_handler() -> StatusCode {
    if health().is_alive() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}
