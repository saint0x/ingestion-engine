//! Application state shared across handlers.

use crate::middleware::rate_limit::{RateLimitConfig, RateLimiter, SharedRateLimiter};
use clickhouse_client::ClickHouseClient;
use engine_core::{AuthRequest, AuthResponse, Error, ParsedApiKey};
use moka::future::Cache;
use redpanda::EventProducer;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

/// Cache TTL for auth responses (30 seconds).
const AUTH_CACHE_TTL: Duration = Duration::from_secs(30);

/// Maximum cache entries.
const AUTH_CACHE_MAX_CAPACITY: u64 = 10_000;

/// Auth service client.
///
/// Calls the TS auth daemon's `/internal/auth/validate` endpoint.
/// Caches responses for 30 seconds to reduce load on auth service.
#[derive(Clone)]
pub struct AuthClient {
    /// Auth service URL (e.g., "http://auth-service:8080")
    base_url: String,
    /// HTTP client
    http_client: reqwest::Client,
    /// Auth response cache (API key hash -> AuthResponse)
    cache: Cache<String, AuthResponse>,
    /// Whether to use mock mode (for testing)
    mock_mode: bool,
}

impl AuthClient {
    /// Creates a new auth client.
    pub fn new(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into();
        let mock_mode = base_url.is_empty() || base_url == "mock";

        Self {
            base_url,
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("Failed to create HTTP client"),
            cache: Cache::builder()
                .max_capacity(AUTH_CACHE_MAX_CAPACITY)
                .time_to_live(AUTH_CACHE_TTL)
                .build(),
            mock_mode,
        }
    }

    /// Validate an API key with the auth service.
    ///
    /// Returns cached response if available, otherwise calls the auth service.
    pub async fn validate(&self, api_key: &ParsedApiKey) -> Result<AuthResponse, Error> {
        let cache_key = api_key.as_str().to_string();

        // Check cache first
        if let Some(cached) = self.cache.get(&cache_key).await {
            debug!("Auth cache hit");
            return Ok(cached);
        }

        // Get response (mock or real)
        let response = if self.mock_mode {
            self.mock_validate(api_key)
        } else {
            self.remote_validate(api_key).await?
        };

        // Cache the response
        self.cache.insert(cache_key, response.clone()).await;

        Ok(response)
    }

    /// Call the remote auth service.
    async fn remote_validate(&self, api_key: &ParsedApiKey) -> Result<AuthResponse, Error> {
        let url = format!("{}/internal/auth/validate", self.base_url);
        let request = AuthRequest::write(api_key.as_str());

        debug!(url = %url, "Calling auth service");

        let response = self
            .http_client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                warn!(error = %e, "Auth service request failed");
                Error::internal(format!("Auth service unavailable: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!(status = %status, body = %body, "Auth service returned error");
            return Err(Error::internal(format!(
                "Auth service returned {}: {}",
                status, body
            )));
        }

        let auth_response: AuthResponse = response.json().await.map_err(|e| {
            warn!(error = %e, "Failed to parse auth response");
            Error::internal(format!("Invalid auth response: {}", e))
        })?;

        Ok(auth_response)
    }

    /// Mock validation for testing/development.
    fn mock_validate(&self, api_key: &ParsedApiKey) -> AuthResponse {
        debug!("Using mock auth validation");
        AuthResponse {
            valid: true,
            project_id: Some(generate_mock_project_id(api_key)),
            permissions: Some(vec!["read".into(), "write".into()]),
            rate_limit: Some(1000),
            allowed_origins: None,
            error: None,
            mau: None,
        }
    }

    /// Invalidate cached auth response for an API key.
    pub async fn invalidate(&self, api_key: &ParsedApiKey) {
        self.cache.invalidate(&api_key.as_str().to_string()).await;
    }

    /// Fetch feature flags for a project.
    ///
    /// In mock mode, returns all features enabled except location.
    pub async fn fetch_features(&self, api_key: &ParsedApiKey) -> Result<Features, Error> {
        if self.mock_mode {
            return Ok(Features::default());
        }

        let url = format!("{}/features", self.base_url);

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", api_key.as_str()))
            .send()
            .await
            .map_err(|e| {
                warn!(error = %e, "Features request failed");
                Error::internal(format!("Features service unavailable: {}", e))
            })?;

        if !response.status().is_success() {
            // Fall back to default features on error
            warn!("Features endpoint returned error, using defaults");
            return Ok(Features::default());
        }

        let features_response: FeaturesResponse = response.json().await.map_err(|e| {
            warn!(error = %e, "Failed to parse features response");
            Error::internal(format!("Invalid features response: {}", e))
        })?;

        Ok(features_response.features)
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

/// Feature flags response from the auth service.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeaturesResponse {
    pub features: Features,
    pub project_id: String,
}

/// Feature flags for a project.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Features {
    // Core analytics features
    #[serde(default = "default_true")]
    pub pageviews: bool,
    #[serde(default = "default_true")]
    pub clicks: bool,
    #[serde(default = "default_true")]
    pub scrolling: bool,
    #[serde(default = "default_true")]
    pub mouse_move: bool,
    #[serde(default = "default_true")]
    pub forms: bool,
    #[serde(default = "default_true")]
    pub performance: bool,
    #[serde(default = "default_true")]
    pub errors: bool,
    #[serde(default = "default_true")]
    pub visibility: bool,
    #[serde(default = "default_true")]
    pub resources: bool,
    #[serde(default)]
    pub location: bool,

    // Overwatch Triggers v1.0 - Context-based notification system
    /// Enable exit intent detection events
    #[serde(default = "default_true")]
    pub exit_intent: bool,
    /// Enable idle detection events (idle_start, idle_end)
    #[serde(default = "default_true")]
    pub idle_detection: bool,
    /// Enable engagement scoring snapshots
    #[serde(default = "default_true")]
    pub engagement_scoring: bool,
    /// Enable trigger lifecycle events (registered, fired, dismissed, action, error)
    #[serde(default = "default_true")]
    pub triggers: bool,
}

fn default_true() -> bool {
    true
}

impl Features {
    /// Check if a given event type is enabled.
    pub fn is_event_enabled(&self, event_type: &str) -> bool {
        match event_type {
            // Core analytics events
            "pageview" | "pageleave" => self.pageviews,
            "click" => self.clicks,
            "scroll" => self.scrolling,
            "mouse_move" => self.mouse_move,
            "form_focus" | "form_blur" | "form_submit" | "form_abandon" => self.forms,
            "performance" => self.performance,
            "error" => self.errors,
            "visibility_change" => self.visibility,
            "resource_load" => self.resources,

            // Overwatch Triggers v1.0
            "exit_intent" => self.exit_intent,
            "idle_start" | "idle_end" => self.idle_detection,
            "engagement_snapshot" => self.engagement_scoring,
            "trigger_registered" | "trigger_fired" | "trigger_dismissed" | "trigger_action" | "trigger_error" => {
                self.triggers
            }

            // session_start, session_end, custom are always enabled
            _ => true,
        }
    }

    /// Check if any Overwatch Triggers feature is enabled.
    pub fn is_triggers_enabled(&self) -> bool {
        self.exit_intent || self.idle_detection || self.engagement_scoring || self.triggers
    }
}

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Event producer (Redpanda in production, mock in tests)
    pub producer: Arc<dyn EventProducer>,
    /// ClickHouse client
    pub clickhouse: Arc<ClickHouseClient>,
    /// Auth service client
    pub auth_client: AuthClient,
    /// Rate limiter
    pub rate_limiter: SharedRateLimiter,
}

impl AppState {
    pub fn new(
        producer: Arc<dyn EventProducer>,
        clickhouse: Arc<ClickHouseClient>,
        auth_url: impl Into<String>,
    ) -> Self {
        Self {
            producer,
            clickhouse,
            auth_client: AuthClient::new(auth_url),
            rate_limiter: Arc::new(RateLimiter::new(RateLimitConfig::default())),
        }
    }

    /// Create with custom rate limit config.
    pub fn with_rate_limit(
        producer: Arc<dyn EventProducer>,
        clickhouse: Arc<ClickHouseClient>,
        auth_url: impl Into<String>,
        rate_config: RateLimitConfig,
    ) -> Self {
        Self {
            producer,
            clickhouse,
            auth_client: AuthClient::new(auth_url),
            rate_limiter: Arc::new(RateLimiter::new(rate_config)),
        }
    }

    /// Start the rate limiter cleanup background task.
    /// Returns a handle that can be used to cancel the task.
    pub fn start_rate_limiter_cleanup(&self) -> tokio::task::JoinHandle<()> {
        let rate_limiter = self.rate_limiter.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
            loop {
                interval.tick().await;
                rate_limiter.cleanup_stale();
            }
        })
    }
}
