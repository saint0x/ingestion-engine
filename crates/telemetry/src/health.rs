//! Health check aggregation.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

/// Health status for a component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

impl HealthStatus {
    pub fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy)
    }

    pub fn is_serving(&self) -> bool {
        matches!(self, Self::Healthy | Self::Degraded)
    }
}

/// Component health state.
#[derive(Debug)]
pub struct ComponentHealth {
    name: &'static str,
    healthy: AtomicBool,
    message: parking_lot::RwLock<Option<String>>,
}

impl ComponentHealth {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            healthy: AtomicBool::new(false),
            message: parking_lot::RwLock::new(None),
        }
    }

    pub fn set_healthy(&self) {
        self.healthy.store(true, Ordering::Relaxed);
        *self.message.write() = None;
    }

    pub fn set_unhealthy(&self, msg: impl Into<String>) {
        self.healthy.store(false, Ordering::Relaxed);
        *self.message.write() = Some(msg.into());
    }

    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Relaxed)
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn message(&self) -> Option<String> {
        self.message.read().clone()
    }
}

/// Aggregated health status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub status: HealthStatus,
    pub components: Vec<ComponentHealthReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealthReport {
    pub name: String,
    pub healthy: bool,
    pub message: Option<String>,
}

/// Global health registry.
pub struct HealthRegistry {
    pub redpanda: ComponentHealth,
    pub clickhouse: ComponentHealth,
}

impl HealthRegistry {
    pub const fn new() -> Self {
        Self {
            redpanda: ComponentHealth::new("redpanda"),
            clickhouse: ComponentHealth::new("clickhouse"),
        }
    }

    /// Generate a health report.
    pub fn report(&self) -> HealthReport {
        let components = vec![
            ComponentHealthReport {
                name: self.redpanda.name().to_string(),
                healthy: self.redpanda.is_healthy(),
                message: self.redpanda.message(),
            },
            ComponentHealthReport {
                name: self.clickhouse.name().to_string(),
                healthy: self.clickhouse.is_healthy(),
                message: self.clickhouse.message(),
            },
        ];

        let all_healthy = components.iter().all(|c| c.healthy);
        let any_healthy = components.iter().any(|c| c.healthy);

        let status = if all_healthy {
            HealthStatus::Healthy
        } else if any_healthy {
            HealthStatus::Degraded
        } else {
            HealthStatus::Unhealthy
        };

        HealthReport { status, components }
    }

    /// Check if the service can accept traffic.
    pub fn is_ready(&self) -> bool {
        self.redpanda.is_healthy()
    }

    /// Check if the service is alive.
    pub fn is_alive(&self) -> bool {
        true // Service is running
    }
}

impl Default for HealthRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global health registry.
pub static HEALTH: std::sync::LazyLock<HealthRegistry> =
    std::sync::LazyLock::new(HealthRegistry::new);

/// Get the global health registry.
pub fn health() -> &'static HealthRegistry {
    &HEALTH
}
