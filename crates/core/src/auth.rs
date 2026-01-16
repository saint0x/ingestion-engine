//! Authentication types and API key validation.
//!
//! This module provides:
//! - API key format validation (owk_live/test_xxx)
//! - Auth request/response types for daemon communication

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

use crate::error::{AuthErrorCode, Error, Result};
use crate::limits::API_KEY_PATTERN;

/// Compiled API key regex (lazy initialization).
static API_KEY_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(API_KEY_PATTERN).expect("invalid API key pattern"));

/// API key environment: live or test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyEnv {
    Live,
    Test,
}

impl ApiKeyEnv {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Test => "test",
        }
    }
}

/// Parsed and validated API key from a request.
///
/// This is distinct from the stored `ApiKey` record in tenant.rs.
#[derive(Debug, Clone)]
pub struct ParsedApiKey {
    /// Raw key string.
    raw: String,
    /// Environment (live/test).
    env: ApiKeyEnv,
}

impl ParsedApiKey {
    /// Parse and validate an API key.
    ///
    /// Format: `owk_(live|test)_[a-zA-Z0-9]{32}`
    pub fn parse(key: &str) -> Result<Self> {
        if key.is_empty() {
            return Err(Error::auth(AuthErrorCode::MissingKey, "API key is required"));
        }

        if !API_KEY_REGEX.is_match(key) {
            return Err(Error::auth(
                AuthErrorCode::InvalidFormat,
                "Invalid API key format",
            ));
        }

        // Extract environment from key
        let env = if key.starts_with("owk_live_") {
            ApiKeyEnv::Live
        } else {
            ApiKeyEnv::Test
        };

        Ok(Self {
            raw: key.to_string(),
            env,
        })
    }

    /// Get the raw key string.
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// Get the key environment.
    pub fn env(&self) -> ApiKeyEnv {
        self.env
    }

    /// Check if this is a live key.
    pub fn is_live(&self) -> bool {
        self.env == ApiKeyEnv::Live
    }

    /// Check if this is a test key.
    pub fn is_test(&self) -> bool {
        self.env == ApiKeyEnv::Test
    }
}

/// Request to auth service.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRequest {
    /// The API key to validate.
    pub api_key: String,
    /// Required permission (typically "write" for ingestion).
    pub required_permission: String,
}

impl AuthRequest {
    /// Create a new auth request for write permission.
    pub fn write(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            required_permission: "write".to_string(),
        }
    }
}

/// Successful auth response from daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    /// Whether the key is valid.
    pub valid: bool,
    /// Project ID associated with the key.
    pub project_id: Option<String>,
    /// Granted permissions.
    pub permissions: Option<Vec<String>>,
    /// Rate limit (requests per minute).
    pub rate_limit: Option<u32>,
    /// Allowed origins for CORS.
    pub allowed_origins: Option<Vec<String>>,
    /// Error details if invalid.
    pub error: Option<AuthResponseError>,
    /// MAU status for the project.
    pub mau: Option<MauStatus>,
}

/// Auth error in response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponseError {
    pub code: String,
    pub message: String,
}

/// MAU (Monthly Active Users) status from daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MauStatus {
    /// MAU limit for the project's plan.
    pub limit: u32,
    /// Whether the project has exceeded its MAU limit.
    pub is_over_limit: bool,
}

impl AuthResponse {
    /// Check if auth was successful and extract project ID.
    pub fn project_id(&self) -> Result<&str> {
        if !self.valid {
            let err = self.error.as_ref();
            let code = err.map(|e| e.code.as_str()).unwrap_or("AUTH_003");
            let msg = err
                .map(|e| e.message.as_str())
                .unwrap_or("Invalid API key");

            return Err(match code {
                "AUTH_001" => Error::auth(AuthErrorCode::MissingKey, msg),
                "AUTH_002" => Error::auth(AuthErrorCode::InvalidFormat, msg),
                "AUTH_003" => Error::auth(AuthErrorCode::InvalidKey, msg),
                "AUTH_004" => Error::auth(AuthErrorCode::Revoked, msg),
                "AUTH_005" => Error::auth(AuthErrorCode::InsufficientPermissions, msg),
                _ => Error::auth(AuthErrorCode::InvalidKey, msg),
            });
        }

        self.project_id
            .as_deref()
            .ok_or_else(|| Error::auth(AuthErrorCode::InvalidKey, "Missing project ID in response"))
    }

    /// Get rate limit or default.
    pub fn rate_limit_or_default(&self) -> u32 {
        self.rate_limit.unwrap_or(1000)
    }
}

/// Extract API key from request headers.
///
/// Checks in order:
/// 1. `Authorization: Bearer <key>`
/// 2. `X-API-Key: <key>`
pub fn extract_api_key(auth_header: Option<&str>, api_key_header: Option<&str>) -> Result<ParsedApiKey> {
    // Try Authorization header first
    if let Some(auth) = auth_header {
        if let Some(key) = auth.strip_prefix("Bearer ") {
            return ParsedApiKey::parse(key.trim());
        }
    }

    // Fall back to X-API-Key header
    if let Some(key) = api_key_header {
        return ParsedApiKey::parse(key.trim());
    }

    Err(Error::auth(AuthErrorCode::MissingKey, "API key is required"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_live_key() {
        let key = ParsedApiKey::parse("owk_live_ABC123xyz789DEF456ghi012JKL345mn").unwrap();
        assert!(key.is_live());
        assert!(!key.is_test());
        assert_eq!(key.env(), ApiKeyEnv::Live);
    }

    #[test]
    fn test_valid_test_key() {
        let key = ParsedApiKey::parse("owk_test_ABC123xyz789DEF456ghi012JKL345mn").unwrap();
        assert!(key.is_test());
        assert!(!key.is_live());
        assert_eq!(key.env(), ApiKeyEnv::Test);
    }

    #[test]
    fn test_invalid_key_format() {
        // Too short
        assert!(ParsedApiKey::parse("owk_live_ABC123").is_err());
        // Wrong prefix
        assert!(ParsedApiKey::parse("key_live_ABC123xyz789DEF456ghi012JKL345mn").is_err());
        // Invalid chars
        assert!(ParsedApiKey::parse("owk_live_ABC123xyz789DEF456ghi012JKL345m!").is_err());
        // Empty
        assert!(ParsedApiKey::parse("").is_err());
    }

    #[test]
    fn test_extract_bearer_token() {
        let key = extract_api_key(
            Some("Bearer owk_live_ABC123xyz789DEF456ghi012JKL345mn"),
            None,
        )
        .unwrap();
        assert!(key.is_live());
    }

    #[test]
    fn test_extract_x_api_key() {
        let key = extract_api_key(
            None,
            Some("owk_test_ABC123xyz789DEF456ghi012JKL345mn"),
        )
        .unwrap();
        assert!(key.is_test());
    }

    #[test]
    fn test_extract_missing_key() {
        let err = extract_api_key(None, None).unwrap_err();
        assert!(matches!(err, Error::Auth { .. }));
    }

    #[test]
    fn test_auth_error_codes() {
        assert_eq!(AuthErrorCode::MissingKey.code(), "AUTH_001");
        assert_eq!(AuthErrorCode::InvalidFormat.code(), "AUTH_002");
        assert_eq!(AuthErrorCode::InvalidKey.code(), "AUTH_003");
        assert_eq!(AuthErrorCode::Revoked.code(), "AUTH_004");
        assert_eq!(AuthErrorCode::InsufficientPermissions.code(), "AUTH_005");
    }

    #[test]
    fn test_auth_response_success() {
        let response = AuthResponse {
            valid: true,
            project_id: Some("proj-123".into()),
            permissions: Some(vec!["read".into(), "write".into()]),
            rate_limit: Some(5000),
            allowed_origins: None,
            error: None,
            mau: None,
        };
        assert_eq!(response.project_id().unwrap(), "proj-123");
        assert_eq!(response.rate_limit_or_default(), 5000);
    }

    #[test]
    fn test_auth_response_failure() {
        let response = AuthResponse {
            valid: false,
            project_id: None,
            permissions: None,
            rate_limit: None,
            allowed_origins: None,
            error: Some(AuthResponseError {
                code: "AUTH_003".into(),
                message: "Invalid API key".into(),
            }),
            mau: None,
        };
        assert!(response.project_id().is_err());
    }
}
