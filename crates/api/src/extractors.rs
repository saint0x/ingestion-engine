//! Request extractors.

use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{header, request::Parts},
};
use engine_core::{extract_api_key, ParsedApiKey};

use crate::response::ApiError;
use crate::state::AppState;

/// Authenticated context from request.
///
/// Contains validated API key and auth response from auth service.
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// Validated API key
    pub api_key: ParsedApiKey,
    /// Project ID from auth response
    pub project_id: String,
    /// Rate limit (requests per minute)
    pub rate_limit: u32,
    /// Allowed origins for CORS
    pub allowed_origins: Option<Vec<String>>,
}

#[async_trait]
impl FromRequestParts<AppState> for AuthContext {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Extract API key from headers
        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok());

        let api_key_header = parts.headers.get("X-API-Key").and_then(|h| h.to_str().ok());

        let api_key = extract_api_key(auth_header, api_key_header)?;

        // Call auth service to validate key
        let auth_response = state.auth_client.validate(&api_key).await?;

        let project_id = auth_response.project_id()?.to_string();

        Ok(AuthContext {
            api_key,
            project_id,
            rate_limit: auth_response.rate_limit_or_default(),
            allowed_origins: auth_response.allowed_origins.clone(),
        })
    }
}

/// Client IP address.
#[derive(Debug, Clone)]
pub struct ClientIp(pub Option<String>);

#[async_trait]
impl<S> FromRequestParts<S> for ClientIp
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Try X-Forwarded-For first (for proxied requests)
        if let Some(xff) = parts.headers.get("X-Forwarded-For") {
            if let Ok(xff_str) = xff.to_str() {
                // Take the first IP in the chain
                if let Some(ip) = xff_str.split(',').next() {
                    return Ok(ClientIp(Some(ip.trim().to_string())));
                }
            }
        }

        // Try X-Real-IP
        if let Some(real_ip) = parts.headers.get("X-Real-IP") {
            if let Ok(ip) = real_ip.to_str() {
                return Ok(ClientIp(Some(ip.to_string())));
            }
        }

        Ok(ClientIp(None))
    }
}
