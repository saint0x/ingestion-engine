//! Ingestion endpoint handler.
//!
//! Accepts SDK events in 3 formats:
//! 1. Array: `[event, event, ...]`
//! 2. Object with events: `{ "events": [...], "metadata": {...} }`
//! 3. Single event: `{ "id": "...", "type": "...", ... }`
//!
//! Transforms to ClickHouse format and sends to Redpanda.

use axum::{body::Bytes, extract::State, Json};
use engine_core::{
    limits::{MAX_BATCH_EVENTS, MAX_BATCH_SIZE_BYTES},
    transform_batch, SDKPayload, ValidationErrorCode,
};
use std::time::Instant;
use telemetry::metrics;
use tracing::{debug, error, info, warn};

use crate::extractors::{AuthContext, ClientIp};
use crate::response::{ApiError, IngestResponse};
use crate::state::AppState;

/// POST /ingest - Primary SDK ingestion endpoint.
///
/// Accepts SDK events in camelCase format, validates, transforms to
/// ClickHouse format (snake_case), and sends to Redpanda for processing.
pub async fn ingest_handler(
    State(state): State<AppState>,
    auth: AuthContext,
    ClientIp(_client_ip): ClientIp,
    body: Bytes,
) -> Result<Json<IngestResponse>, ApiError> {
    let start = Instant::now();

    metrics().batches_received.inc();

    // Check payload size before parsing
    if body.len() > MAX_BATCH_SIZE_BYTES {
        return Err(ApiError::validation(
            ValidationErrorCode::BatchTooLarge.code(),
            vec![format!(
                "Payload size {}KB exceeds {}KB limit",
                body.len() / 1024,
                MAX_BATCH_SIZE_BYTES / 1024
            )],
        ));
    }

    debug!(
        project_id = %auth.project_id,
        payload_size = body.len(),
        "Received event batch"
    );

    // Parse SDK payload (supports 3 formats)
    let payload = SDKPayload::parse(&body).map_err(|e| {
        error!("Failed to parse SDK payload: {}", e);
        ApiError::bad_request(e.to_string())
    })?;

    let total_events = payload.events.len();
    metrics().events_received.inc_by(total_events as u64);

    // Check batch size limit
    if total_events > MAX_BATCH_EVENTS {
        return Err(ApiError::validation(
            ValidationErrorCode::BatchTooLarge.code(),
            vec![format!(
                "Batch has {} events, exceeds {} limit",
                total_events, MAX_BATCH_EVENTS
            )],
        ));
    }

    // Transform SDK events to ClickHouse format
    let (ch_events, transform_errors) = transform_batch(payload.events, &auth.project_id)
        .map_err(|e| {
            error!("Transform failed: {}", e);
            ApiError::internal("Failed to process events")
        })?;

    let accepted = ch_events.len();
    let rejected = transform_errors.len();

    if rejected > 0 {
        warn!(
            project_id = %auth.project_id,
            accepted = accepted,
            rejected = rejected,
            "Some events failed validation"
        );
        metrics().events_failed_validation.inc_by(rejected as u64);
    }

    metrics().events_validated.inc_by(accepted as u64);

    // Send to Redpanda
    if !ch_events.is_empty() {
        let send_result = state
            .producer
            .send_clickhouse_events(ch_events)
            .await
            .map_err(|e| {
                error!("Failed to send events to Redpanda: {}", e);
                ApiError::internal("Failed to store events")
            })?;

        if !send_result.errors.is_empty() {
            warn!(
                project_id = %auth.project_id,
                send_errors = ?send_result.errors,
                "Some events failed to send"
            );
        }
    }

    let latency_ms = start.elapsed().as_millis() as u64;
    metrics().ingest_latency_ms.observe(latency_ms);

    info!(
        project_id = %auth.project_id,
        accepted = accepted,
        rejected = rejected,
        latency_ms = latency_ms,
        "Batch processed"
    );

    // Return response with partial errors if any
    if rejected > 0 {
        let error_msgs: Vec<String> = transform_errors
            .into_iter()
            .map(|e| e.to_string())
            .collect();
        Ok(Json(IngestResponse::partial(accepted, error_msgs)))
    } else {
        Ok(Json(IngestResponse::success(accepted)))
    }
}
