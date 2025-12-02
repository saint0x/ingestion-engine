//! Standardized API responses.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

/// Success response for ingestion (matches spec).
#[derive(Debug, Serialize, Deserialize)]
pub struct IngestResponse {
    pub success: bool,
    pub received: usize,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<String>>,
}

impl IngestResponse {
    pub fn success(received: usize) -> Self {
        Self {
            success: true,
            received,
            timestamp: chrono::Utc::now().timestamp_millis(),
            errors: None,
        }
    }

    pub fn partial(received: usize, errors: Vec<String>) -> Self {
        Self {
            success: true,
            received,
            timestamp: chrono::Utc::now().timestamp_millis(),
            errors: if errors.is_empty() { None } else { Some(errors) },
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

/// API error type with spec error codes.
pub struct ApiError {
    pub status: StatusCode,
    pub response: ErrorResponse,
    pub retry_after: Option<u64>,
}

impl ApiError {
    pub fn with_code(status: StatusCode, code: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            status,
            response: ErrorResponse::new(msg, code),
            retry_after: None,
        }
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::with_code(StatusCode::BAD_REQUEST, "VALID_001", msg)
    }

    pub fn unauthorized(code: impl Into<String>, msg: impl Into<String>) -> Self {
        Self::with_code(StatusCode::UNAUTHORIZED, code, msg)
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::with_code(StatusCode::FORBIDDEN, "AUTH_005", msg)
    }

    pub fn rate_limited(msg: impl Into<String>, retry_after: Option<u64>) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            response: ErrorResponse::new(msg, "RATE_001"),
            retry_after,
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::with_code(StatusCode::INTERNAL_SERVER_ERROR, "DB_001", msg)
    }

    pub fn validation(code: impl Into<String>, errors: Vec<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            response: ErrorResponse::new("Validation failed", code)
                .with_details(errors),
            retry_after: None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(self.response)).into_response();

        // Add Retry-After header for rate limit responses
        if let Some(retry_after) = self.retry_after {
            if let Ok(value) = retry_after.to_string().parse() {
                response.headers_mut().insert("Retry-After", value);
            }
        }

        response
    }
}

impl From<engine_core::Error> for ApiError {
    fn from(err: engine_core::Error) -> Self {
        match &err {
            engine_core::Error::Auth { code, message, http_status } => {
                let status = StatusCode::from_u16(*http_status)
                    .unwrap_or(StatusCode::UNAUTHORIZED);
                ApiError::with_code(status, *code, message)
            }
            engine_core::Error::ValidationWithCode { code, message, .. } => {
                ApiError::validation(*code, vec![message.clone()])
            }
            engine_core::Error::Database { code, message, .. } => {
                ApiError::with_code(StatusCode::INTERNAL_SERVER_ERROR, *code, message)
            }
            engine_core::Error::RateLimit { message, retry_after, .. } => {
                ApiError::rate_limited(message, *retry_after)
            }
            engine_core::Error::Validation(msg) => ApiError::bad_request(msg),
            engine_core::Error::Unauthorized(msg) => ApiError::unauthorized("AUTH_003", msg),
            engine_core::Error::RateLimited(msg) => ApiError::rate_limited(msg, None),
            engine_core::Error::InvalidTenant(msg) => ApiError::unauthorized("AUTH_003", msg),
            _ => ApiError::internal(err.to_string()),
        }
    }
}
