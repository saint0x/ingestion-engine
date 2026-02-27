//! Authentication middleware.

use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use tracing::warn;

/// Simple API key validation middleware.
/// In production, this would validate against a database/cache.
pub async fn validate_api_key(request: Request, next: Next) -> Result<Response, StatusCode> {
    // Check for API key in headers
    let has_auth = request.headers().contains_key("Authorization")
        || request.headers().contains_key("X-API-Key");

    if !has_auth {
        warn!("Request missing authentication");
        return Err(StatusCode::UNAUTHORIZED);
    }

    // TODO: Validate API key against tenant database
    // For now, just pass through if header exists

    Ok(next.run(request).await)
}
