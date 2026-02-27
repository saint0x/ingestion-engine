//! Unified error types for the ingestion engine.
//!
//! Error codes follow the spec:
//! - AUTH_001-005: Authentication errors
//! - VALID_001-003: Validation errors
//! - DB_001: Database errors
//! - RATE_001: Rate limit errors

use thiserror::Error;

/// Result type alias using our Error type.
pub type Result<T> = std::result::Result<T, Error>;

/// Authentication error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthErrorCode {
    /// AUTH_001: API key is required
    MissingKey,
    /// AUTH_002: Invalid API key format
    InvalidFormat,
    /// AUTH_003: Invalid API key (not found)
    InvalidKey,
    /// AUTH_004: API key has been revoked
    Revoked,
    /// AUTH_005: Insufficient permissions
    InsufficientPermissions,
}

impl AuthErrorCode {
    /// Get the error code string.
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingKey => "AUTH_001",
            Self::InvalidFormat => "AUTH_002",
            Self::InvalidKey => "AUTH_003",
            Self::Revoked => "AUTH_004",
            Self::InsufficientPermissions => "AUTH_005",
        }
    }

    /// Get the HTTP status code.
    pub fn http_status(&self) -> u16 {
        match self {
            Self::MissingKey => 401,
            Self::InvalidFormat => 401,
            Self::InvalidKey => 401,
            Self::Revoked => 401,
            Self::InsufficientPermissions => 403,
        }
    }
}

/// Validation error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationErrorCode {
    /// VALID_001: Invalid JSON / Invalid format
    InvalidFormat,
    /// VALID_002: Batch exceeds 1000 events
    BatchTooLarge,
    /// VALID_003: Event exceeds 64KB
    EventTooLarge,
}

impl ValidationErrorCode {
    /// Get the error code string.
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidFormat => "VALID_001",
            Self::BatchTooLarge => "VALID_002",
            Self::EventTooLarge => "VALID_003",
        }
    }

    /// Get the HTTP status code.
    pub fn http_status(&self) -> u16 {
        400
    }
}

/// Database error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbErrorCode {
    /// DB_001: Failed to store events
    StoreFailed,
}

impl DbErrorCode {
    /// Get the error code string.
    pub fn code(&self) -> &'static str {
        match self {
            Self::StoreFailed => "DB_001",
        }
    }

    /// Get the HTTP status code.
    pub fn http_status(&self) -> u16 {
        500
    }
}

/// Rate limit error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitErrorCode {
    /// RATE_001: Rate limit exceeded
    Exceeded,
}

impl RateLimitErrorCode {
    /// Get the error code string.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Exceeded => "RATE_001",
        }
    }

    /// Get the HTTP status code.
    pub fn http_status(&self) -> u16 {
        429
    }
}

/// Unified error type for the ingestion engine.
#[derive(Debug, Error)]
pub enum Error {
    /// Authentication error with code.
    #[error("[{code}] {message}")]
    Auth {
        code: &'static str,
        message: String,
        http_status: u16,
    },

    /// Validation error with code.
    #[error("[{code}] {message}")]
    ValidationWithCode {
        code: &'static str,
        message: String,
        http_status: u16,
    },

    /// Database error with code.
    #[error("[{code}] {message}")]
    Database {
        code: &'static str,
        message: String,
        http_status: u16,
    },

    /// Rate limit error with code.
    #[error("[{code}] {message}")]
    RateLimit {
        code: &'static str,
        message: String,
        http_status: u16,
        retry_after: Option<u64>,
    },

    #[error("validation error: {0}")]
    Validation(String),

    #[error("schema error: {0}")]
    Schema(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("invalid event type: {0}")]
    InvalidEventType(String),

    #[error("missing required field: {0}")]
    MissingField(String),

    #[error("invalid tenant: {0}")]
    InvalidTenant(String),

    #[error("invalid session: {0}")]
    InvalidSession(String),

    #[error("rate limited: {0}")]
    RateLimited(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl Error {
    /// Create an authentication error.
    pub fn auth(code: AuthErrorCode, msg: impl Into<String>) -> Self {
        Self::Auth {
            code: code.code(),
            message: msg.into(),
            http_status: code.http_status(),
        }
    }

    /// Create a validation error with code.
    pub fn validation_code(code: ValidationErrorCode, msg: impl Into<String>) -> Self {
        Self::ValidationWithCode {
            code: code.code(),
            message: msg.into(),
            http_status: code.http_status(),
        }
    }

    /// Create a database error.
    pub fn database(code: DbErrorCode, msg: impl Into<String>) -> Self {
        Self::Database {
            code: code.code(),
            message: msg.into(),
            http_status: code.http_status(),
        }
    }

    /// Create a rate limit error.
    pub fn rate_limit(
        code: RateLimitErrorCode,
        msg: impl Into<String>,
        retry_after: Option<u64>,
    ) -> Self {
        Self::RateLimit {
            code: code.code(),
            message: msg.into(),
            http_status: code.http_status(),
            retry_after,
        }
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn schema(msg: impl Into<String>) -> Self {
        Self::Schema(msg.into())
    }

    pub fn missing_field(field: impl Into<String>) -> Self {
        Self::MissingField(field.into())
    }

    pub fn invalid_tenant(msg: impl Into<String>) -> Self {
        Self::InvalidTenant(msg.into())
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::Unauthorized(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    /// Get the HTTP status code for this error.
    pub fn http_status(&self) -> u16 {
        match self {
            Self::Auth { http_status, .. } => *http_status,
            Self::ValidationWithCode { http_status, .. } => *http_status,
            Self::Database { http_status, .. } => *http_status,
            Self::RateLimit { http_status, .. } => *http_status,
            Self::Validation(_) => 400,
            Self::Schema(_) => 400,
            Self::Serialization(_) => 400,
            Self::InvalidEventType(_) => 400,
            Self::MissingField(_) => 400,
            Self::InvalidTenant(_) => 400,
            Self::InvalidSession(_) => 400,
            Self::RateLimited(_) => 429,
            Self::Unauthorized(_) => 401,
            Self::Internal(_) => 500,
        }
    }

    /// Get the error code if this is a coded error.
    pub fn error_code(&self) -> Option<&'static str> {
        match self {
            Self::Auth { code, .. } => Some(code),
            Self::ValidationWithCode { code, .. } => Some(code),
            Self::Database { code, .. } => Some(code),
            Self::RateLimit { code, .. } => Some(code),
            _ => None,
        }
    }
}
