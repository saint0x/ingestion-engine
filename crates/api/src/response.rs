//! Standardized API responses.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Success response for ingestion.
#[derive(Debug, Serialize, Deserialize)]
pub struct IngestResponse {
    pub status: &'static str,
    pub batch_id: Uuid,
    pub events_accepted: usize,
    pub events_rejected: usize,
    pub ingest_latency_ms: u64,
}

impl IngestResponse {
    pub fn success(batch_id: Uuid, accepted: usize, rejected: usize, latency_ms: u64) -> Self {
        Self {
            status: "success",
            batch_id,
            events_accepted: accepted,
            events_rejected: rejected,
            ingest_latency_ms: latency_ms,
        }
    }
}

/// Health check response.
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub redpanda_connected: bool,
    pub clickhouse_connected: bool,
    pub queue_depth: u64,
}

/// Error response.
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Vec<String>>,
}

impl ErrorResponse {
    pub fn new(error: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            code: code.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: Vec<String>) -> Self {
        self.details = Some(details);
        self
    }
}

/// API error type.
pub struct ApiError {
    pub status: StatusCode,
    pub response: ErrorResponse,
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            response: ErrorResponse::new(msg, "BAD_REQUEST"),
        }
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            response: ErrorResponse::new(msg, "UNAUTHORIZED"),
        }
    }

    pub fn rate_limited(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            response: ErrorResponse::new(msg, "RATE_LIMITED"),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            response: ErrorResponse::new(msg, "INTERNAL_ERROR"),
        }
    }

    pub fn validation(errors: Vec<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            response: ErrorResponse::new("Validation failed", "VALIDATION_ERROR")
                .with_details(errors),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.response)).into_response()
    }
}

impl From<engine_core::Error> for ApiError {
    fn from(err: engine_core::Error) -> Self {
        match err {
            engine_core::Error::Validation(msg) => ApiError::bad_request(msg),
            engine_core::Error::Unauthorized(msg) => ApiError::unauthorized(msg),
            engine_core::Error::RateLimited(msg) => ApiError::rate_limited(msg),
            engine_core::Error::InvalidTenant(msg) => ApiError::unauthorized(msg),
            _ => ApiError::internal(err.to_string()),
        }
    }
}
