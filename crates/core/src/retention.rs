//! Retention policy definitions.

use serde::{Deserialize, Serialize};

/// Retention tier for tenants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RetentionTier {
    #[default]
    Free,
    Paid,
    Enterprise,
}

impl RetentionTier {
    /// Raw event retention in hours.
    pub fn raw_event_retention_hours(&self) -> u64 {
        match self {
            Self::Free => 24,             // 24 hours
            Self::Paid => 90 * 24,        // 90 days
            Self::Enterprise => 365 * 24, // 1 year default, configurable
        }
    }

    /// Aggregate retention in hours.
    pub fn aggregate_retention_hours(&self) -> u64 {
        match self {
            Self::Free => 7 * 24,             // 7 days
            Self::Paid => 365 * 24,           // 1 year
            Self::Enterprise => 3 * 365 * 24, // 3 years default
        }
    }

    /// Hours before compression kicks in.
    pub fn compression_after_hours(&self) -> u64 {
        match self {
            Self::Free => 24,            // After 24h
            Self::Paid => 30 * 24,       // After 30 days
            Self::Enterprise => 90 * 24, // After 90 days
        }
    }

    /// Default rate limit (events per second).
    pub fn default_rate_limit(&self) -> u32 {
        match self {
            Self::Free => 100,
            Self::Paid => 10_000,
            Self::Enterprise => 100_000,
        }
    }
}

/// Retention policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// Tier this policy applies to
    pub tier: RetentionTier,
    /// Raw event retention override (hours)
    pub raw_retention_hours: Option<u64>,
    /// Aggregate retention override (hours)
    pub aggregate_retention_hours: Option<u64>,
    /// Compression delay override (hours)
    pub compression_after_hours: Option<u64>,
}

impl RetentionPolicy {
    /// Creates a policy from a tier with default values.
    pub fn from_tier(tier: RetentionTier) -> Self {
        Self {
            tier,
            raw_retention_hours: None,
            aggregate_retention_hours: None,
            compression_after_hours: None,
        }
    }

    /// Returns the effective raw retention hours.
    pub fn effective_raw_retention(&self) -> u64 {
        self.raw_retention_hours
            .unwrap_or_else(|| self.tier.raw_event_retention_hours())
    }

    /// Returns the effective aggregate retention hours.
    pub fn effective_aggregate_retention(&self) -> u64 {
        self.aggregate_retention_hours
            .unwrap_or_else(|| self.tier.aggregate_retention_hours())
    }

    /// Returns the effective compression delay hours.
    pub fn effective_compression_delay(&self) -> u64 {
        self.compression_after_hours
            .unwrap_or_else(|| self.tier.compression_after_hours())
    }
}
