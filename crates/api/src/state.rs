//! Application state shared across handlers.

use clickhouse_client::ClickHouseClient;
use engine_core::{AuthResponse, Error, ParsedApiKey};
use redpanda::Producer;
use std::sync::Arc;

/// Auth service client.
///
/// In production, this calls the TS auth daemon.
/// For now, provides a mock implementation.
#[derive(Clone)]
pub struct AuthClient {
    /// Auth service URL (e.g., "http://auth-service:8080")
    #[allow(dead_code)]
    base_url: String,
}

impl AuthClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }

    /// Validate an API key with the auth service.
    pub async fn validate(&self, api_key: &ParsedApiKey) -> Result<AuthResponse, Error> {
        // TODO: In production, this calls the TS auth daemon:
        // POST {base_url}/internal/auth/validate
        // Body: { "apiKey": "owk_...", "requiredPermission": "write" }
        //
        // For now, return a mock successful response for valid key formats.
        // The key format is already validated by ParsedApiKey::parse().

        // Mock implementation - always succeed for valid format keys
        Ok(AuthResponse {
            valid: true,
            project_id: Some(generate_mock_project_id(api_key)),
            permissions: Some(vec!["read".into(), "write".into()]),
            rate_limit: Some(1000),
            allowed_origins: None,
            error: None,
        })
    }
}

/// Generate a deterministic mock project ID from the API key.
/// This is for testing only - in production, the auth service provides this.
fn generate_mock_project_id(api_key: &ParsedApiKey) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    api_key.as_str().hash(&mut hasher);
    let hash = hasher.finish();
    format!("proj-{:016x}", hash)
}

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Redpanda producer
    pub producer: Arc<Producer>,
    /// ClickHouse client
    pub clickhouse: Arc<ClickHouseClient>,
    /// Auth service client
    pub auth_client: AuthClient,
}

impl AppState {
    pub fn new(
        producer: Arc<Producer>,
        clickhouse: Arc<ClickHouseClient>,
        auth_url: impl Into<String>,
    ) -> Self {
        Self {
            producer,
            clickhouse,
            auth_client: AuthClient::new(auth_url),
        }
    }
}
