//! Notification worker for admin alerts.

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Notification types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Notification {
    /// High error rate alert
    HighErrorRate {
        tenant_id: String,
        error_rate: f64,
        threshold: f64,
    },
    /// Quota warning
    QuotaWarning {
        tenant_id: String,
        usage_percent: f64,
    },
    /// Retention approaching
    RetentionWarning {
        tenant_id: String,
        days_remaining: u32,
    },
    /// System alert
    SystemAlert { message: String, severity: String },
}

/// Notification channel.
#[derive(Debug, Clone)]
pub enum NotificationChannel {
    /// Log only (default)
    Log,
    /// Webhook
    Webhook { url: String },
    /// Email (placeholder)
    Email { to: String },
}

/// Notification worker.
pub struct NotificationWorker {
    channels: Vec<NotificationChannel>,
}

impl Default for NotificationWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationWorker {
    pub fn new() -> Self {
        Self {
            channels: vec![NotificationChannel::Log],
        }
    }

    pub fn with_channel(mut self, channel: NotificationChannel) -> Self {
        self.channels.push(channel);
        self
    }

    /// Send a notification.
    pub async fn send(&self, notification: Notification) -> Result<(), String> {
        for channel in &self.channels {
            match channel {
                NotificationChannel::Log => {
                    info!(notification = ?notification, "Notification");
                }
                NotificationChannel::Webhook { url } => {
                    // Would POST to webhook URL
                    info!(url = url, "Would send webhook");
                }
                NotificationChannel::Email { to } => {
                    // Would send email
                    info!(to = to, "Would send email");
                }
            }
        }

        Ok(())
    }

    /// Check metrics and send alerts if thresholds exceeded.
    pub async fn check_and_alert(&self) -> Result<(), String> {
        use telemetry::metrics;

        let snapshot = metrics().snapshot();

        // Check error rate
        let total = snapshot.events_received;
        let failed = snapshot.events_failed_validation;
        if total > 0 {
            let error_rate = failed as f64 / total as f64;
            if error_rate > 0.1 {
                // 10% threshold
                self.send(Notification::SystemAlert {
                    message: format!("High validation error rate: {:.2}%", error_rate * 100.0),
                    severity: "warning".to_string(),
                })
                .await?;
            }
        }

        // Check backpressure
        if snapshot.backpressure_active {
            self.send(Notification::SystemAlert {
                message: "Backpressure active - system under load".to_string(),
                severity: "warning".to_string(),
            })
            .await?;
        }

        Ok(())
    }
}
