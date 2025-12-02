//! Request extractors.

use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{header, request::Parts, StatusCode},
};
use uuid::Uuid;

use crate::response::ApiError;

/// Extracted tenant context from request.
#[derive(Debug, Clone)]
pub struct TenantId(pub Uuid);

#[async_trait]
impl<S> FromRequestParts<S> for TenantId
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Try X-Tenant-ID header first
        if let Some(tenant_header) = parts.headers.get("X-Tenant-ID") {
            let tenant_str = tenant_header
                .to_str()
                .map_err(|_| ApiError::bad_request("Invalid X-Tenant-ID header"))?;

            let tenant_id = Uuid::parse_str(tenant_str)
                .map_err(|_| ApiError::bad_request("Invalid tenant ID format"))?;

            return Ok(TenantId(tenant_id));
        }

        // Try to extract from API key (format: tenant_id:key)
        if let Some(auth_header) = parts.headers.get(header::AUTHORIZATION) {
            let auth_str = auth_header
                .to_str()
                .map_err(|_| ApiError::bad_request("Invalid Authorization header"))?;

            // Expect "Bearer <api_key>" format
            let token = auth_str
                .strip_prefix("Bearer ")
                .ok_or_else(|| ApiError::unauthorized("Invalid authorization format"))?;

            // API key format: tenant_id:secret
            if let Some((tenant_part, _)) = token.split_once(':') {
                let tenant_id = Uuid::parse_str(tenant_part)
                    .map_err(|_| ApiError::unauthorized("Invalid API key format"))?;
                return Ok(TenantId(tenant_id));
            }
        }

        Err(ApiError::unauthorized("Missing tenant identification"))
    }
}

/// Extracted API key from request.
#[derive(Debug, Clone)]
pub struct ApiKey(pub String);

#[async_trait]
impl<S> FromRequestParts<S> for ApiKey
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Try Authorization header
        if let Some(auth_header) = parts.headers.get(header::AUTHORIZATION) {
            let auth_str = auth_header
                .to_str()
                .map_err(|_| ApiError::bad_request("Invalid Authorization header"))?;

            let token = auth_str
                .strip_prefix("Bearer ")
                .ok_or_else(|| ApiError::unauthorized("Invalid authorization format"))?;

            return Ok(ApiKey(token.to_string()));
        }

        // Try X-API-Key header
        if let Some(key_header) = parts.headers.get("X-API-Key") {
            let key = key_header
                .to_str()
                .map_err(|_| ApiError::bad_request("Invalid X-API-Key header"))?;

            return Ok(ApiKey(key.to_string()));
        }

        Err(ApiError::unauthorized("Missing API key"))
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
