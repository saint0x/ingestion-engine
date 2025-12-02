//! Ingestion endpoint handler.

use axum::{extract::State, Json};
use engine_core::{schema::validate_batch, EventBatch};
use telemetry::metrics;
use std::time::Instant;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::extractors::{ClientIp, TenantId};
use crate::response::{ApiError, IngestResponse};
use crate::state::AppState;

/// POST /ingest - Primary SDK ingestion endpoint.
pub async fn ingest_handler(
    State(state): State<AppState>,
    TenantId(tenant_id): TenantId,
    ClientIp(client_ip): ClientIp,
    Json(mut batch): Json<EventBatch>,
) -> Result<Json<IngestResponse>, ApiError> {
    let start = Instant::now();
    let batch_id = Uuid::new_v4();
    let total_events = batch.events.len();

    metrics().batches_received.inc();
    metrics().events_received.inc_by(total_events as u64);

    debug!(
        batch_id = %batch_id,
        tenant_id = %tenant_id,
        events = total_events,
        "Received event batch"
    );

    // Validate batch
    let validation_errors = validate_batch(&batch).map_err(|e| {
        error!("Batch validation failed: {}", e);
        ApiError::bad_request(e.to_string())
    })?;

    let rejected = validation_errors.len();
    if rejected > 0 {
        warn!(
            batch_id = %batch_id,
            rejected = rejected,
            "Some events failed validation"
        );
        metrics().events_failed_validation.inc_by(rejected as u64);
    }

    // Enrich events with server-side data
    for event in &mut batch.events {
        // Ensure tenant_id matches
        if event.tenant_id != tenant_id {
            event.tenant_id = tenant_id;
        }

        // Add client IP to metadata
        if let Some(ref mut meta) = event.metadata {
            if meta.ip.is_none() {
                meta.ip = client_ip.clone();
            }
        }

        // Set received_at timestamp
        event.received_at = chrono::Utc::now();
    }

    // Filter out invalid events
    let valid_events: Vec<_> = batch
        .events
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !validation_errors.iter().any(|e| e.to_string().contains(&format!("event[{}]", i))))
        .map(|(_, e)| e)
        .collect();

    let accepted = valid_events.len();
    metrics().events_validated.inc_by(accepted as u64);

    // Send to Redpanda
    if !valid_events.is_empty() {
        let send_result = state.producer.send_many(valid_events).await.map_err(|e| {
            error!("Failed to send events to Redpanda: {}", e);
            ApiError::internal("Failed to process events")
        })?;

        if !send_result.errors.is_empty() {
            warn!(
                batch_id = %batch_id,
                errors = ?send_result.errors,
                "Some events failed to send"
            );
        }
    }

    let latency_ms = start.elapsed().as_millis() as u64;
    metrics().ingest_latency_ms.observe(latency_ms);

    info!(
        batch_id = %batch_id,
        tenant_id = %tenant_id,
        accepted = accepted,
        rejected = rejected,
        latency_ms = latency_ms,
        "Batch processed"
    );

    Ok(Json(IngestResponse::success(
        batch_id,
        accepted,
        rejected,
        latency_ms,
    )))
}
