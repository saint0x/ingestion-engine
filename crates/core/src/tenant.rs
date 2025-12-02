//! Tenant and API key management types.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::retention::RetentionTier;

/// A tenant/project in the system.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Tenant {
    /// Unique tenant ID
    pub id: Uuid,
    /// Display name
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    /// Retention tier
    pub tier: RetentionTier,
    /// Whether the tenant is active
    pub active: bool,
    /// Rate limit override (events per second)
    pub rate_limit: Option<u32>,
}

/// API key for authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    /// The API key value (hashed in storage)
    pub key_hash: String,
    /// Associated tenant ID
    pub tenant_id: Uuid,
    /// Human-readable name
    pub name: String,
    /// Whether the key is active
    pub active: bool,
    /// Optional expiration
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Tenant {
    /// Creates a new tenant with default settings.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            tier: RetentionTier::Free,
            active: true,
            rate_limit: None,
        }
    }

    /// Returns the effective rate limit for this tenant.
    pub fn effective_rate_limit(&self) -> u32 {
        self.rate_limit.unwrap_or_else(|| self.tier.default_rate_limit())
    }
}

/// Validated tenant context extracted from request.
#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant: Tenant,
    pub api_key_name: String,
}
