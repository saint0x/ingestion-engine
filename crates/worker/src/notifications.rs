//! Notification worker for admin alerts.

use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info, warn};

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
    /// Merge pressure alert (from ClickHouse ops monitoring)
    MergePressure {
        score: f64,
        max_parts: u64,
        active_merges: usize,
    },
    /// Disk usage alert
    DiskUsage {
        disk_name: String,
        usage_percent: f64,
        free_gb: u64,
    },
}

/// Webhook payload format.
#[derive(Debug, Serialize)]
struct WebhookPayload<'a> {
    timestamp: String,
    source: &'static str,
    notification: &'a Notification,
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
    http_client: Client,
}

impl Default for NotificationWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationWorker {
    pub fn new() -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            channels: vec![NotificationChannel::Log],
            http_client,
        }
    }

    /// Create a notification worker from environment configuration.
    pub fn from_env() -> Self {
        let mut worker = Self::new();

        // Check for webhook URL in environment
        if let Ok(url) = std::env::var("INGESTION_NOTIFICATION_WEBHOOK_URL") {
            if !url.is_empty() {
                info!(url = %url, "Webhook notifications enabled");
                worker = worker.with_channel(NotificationChannel::Webhook { url });
            }
        }

        worker
    }

    pub fn with_channel(mut self, channel: NotificationChannel) -> Self {
        self.channels.push(channel);
        self
    }

    /// Send a notification to all configured channels.
    pub async fn send(&self, notification: Notification) -> Result<(), String> {
        for channel in &self.channels {
            match channel {
                NotificationChannel::Log => {
                    self.log_notification(&notification);
                }
                NotificationChannel::Webhook { url } => {
                    if let Err(e) = self.send_webhook(url, &notification).await {
                        error!(url = url, error = %e, "Failed to send webhook");
                    }
                }
                NotificationChannel::Email { to } => {
                    // Email not implemented - just log
                    info!(to = to, notification = ?notification, "Would send email (not implemented)");
                }
            }
        }

        Ok(())
    }

    /// Log a notification with appropriate severity.
    fn log_notification(&self, notification: &Notification) {
        match notification {
            Notification::HighErrorRate {
                tenant_id,
                error_rate,
                threshold,
            } => {
                warn!(
                    tenant_id = tenant_id,
                    error_rate = error_rate,
                    threshold = threshold,
                    "High error rate detected"
                );
            }
            Notification::QuotaWarning {
                tenant_id,
                usage_percent,
            } => {
                warn!(
                    tenant_id = tenant_id,
                    usage_percent = usage_percent,
                    "Quota warning"
                );
            }
            Notification::RetentionWarning {
                tenant_id,
                days_remaining,
            } => {
                info!(
                    tenant_id = tenant_id,
                    days_remaining = days_remaining,
                    "Retention warning"
                );
            }
            Notification::SystemAlert { message, severity } => match severity.as_str() {
                "critical" | "error" => error!(message = message, "System alert"),
                "warning" => warn!(message = message, "System alert"),
                _ => info!(message = message, "System alert"),
            },
            Notification::MergePressure {
                score,
                max_parts,
                active_merges,
            } => {
                if *score > 70.0 {
                    error!(
                        score = score,
                        max_parts = max_parts,
                        active_merges = active_merges,
                        "CRITICAL: ClickHouse merge pressure"
                    );
                } else {
                    warn!(
                        score = score,
                        max_parts = max_parts,
                        active_merges = active_merges,
                        "ClickHouse merge pressure elevated"
                    );
                }
            }
            Notification::DiskUsage {
                disk_name,
                usage_percent,
                free_gb,
            } => {
                warn!(
                    disk = disk_name,
                    usage_percent = usage_percent,
                    free_gb = free_gb,
                    "Disk usage high"
                );
            }
        }
    }

    /// Send notification via webhook.
    async fn send_webhook(&self, url: &str, notification: &Notification) -> Result<(), String> {
        let payload = WebhookPayload {
            timestamp: Utc::now().to_rfc3339(),
            source: "overwatch-ingestion",
            notification,
        };

        match self.http_client.post(url).json(&payload).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    debug!(url = url, "Webhook sent successfully");
                    Ok(())
                } else {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    warn!(
                        url = url,
                        status = %status,
                        body = body,
                        "Webhook returned error status"
                    );
                    Err(format!("Webhook returned status {}", status))
                }
            }
            Err(e) => {
                error!(url = url, error = %e, "Failed to send webhook");
                Err(format!("Webhook error: {}", e))
            }
        }
    }

    /// Check metrics and send alerts if thresholds exceeded.
    pub async fn check_and_alert(&self) -> Result<(), String> {
        use telemetry::metrics;

        let snapshot = metrics().snapshot();

        // Check error rate
        let total = snapshot.events_received;
        let failed = snapshot.events_failed_validation;
        if total > 100 {
            // Only alert after meaningful sample size
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

    /// Check ClickHouse operational metrics and alert if needed.
    pub async fn check_clickhouse_ops(
        &self,
        ops: &clickhouse_client::ClickHouseOpsMetrics,
    ) -> Result<(), String> {
        // Alert on critical merge pressure
        if ops.is_merge_pressure_critical() {
            self.send(Notification::MergePressure {
                score: ops.merge_pressure_score,
                max_parts: ops.max_parts_count,
                active_merges: ops.total_active_merges,
            })
            .await?;
        }

        // Alert on high disk usage
        for (disk, usage_pct) in ops.high_usage_disks() {
            self.send(Notification::DiskUsage {
                disk_name: disk.name.clone(),
                usage_percent: usage_pct,
                free_gb: disk.free_space / 1_000_000_000,
            })
            .await?;
        }

        Ok(())
    }
}
