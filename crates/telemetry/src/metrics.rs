//! Internal metrics collection.
//!
//! Collects metrics in-memory and periodically flushes to ClickHouse.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// A counter metric.
#[derive(Debug, Default)]
pub struct Counter(AtomicU64);

impl Counter {
    pub fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    pub fn inc(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_by(&self, n: u64) {
        self.0.fetch_add(n, Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    pub fn reset(&self) -> u64 {
        self.0.swap(0, Ordering::Relaxed)
    }
}

/// A gauge metric (can go up or down).
#[derive(Debug, Default)]
pub struct Gauge(AtomicU64);

impl Gauge {
    pub fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    pub fn set(&self, val: u64) {
        self.0.store(val, Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    pub fn inc(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec(&self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Histogram for latency tracking.
#[derive(Debug)]
pub struct Histogram {
    /// Buckets: 1ms, 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 500ms, 1s, 5s, 10s
    buckets: [AtomicU64; 11],
    sum: AtomicU64,
    count: AtomicU64,
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

impl Histogram {
    const BUCKET_BOUNDS: [u64; 11] = [1, 5, 10, 25, 50, 100, 250, 500, 1000, 5000, 10000];

    pub fn new() -> Self {
        Self {
            buckets: Default::default(),
            sum: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }

    /// Records a value in milliseconds.
    pub fn observe(&self, ms: u64) {
        self.sum.fetch_add(ms, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);

        for (i, &bound) in Self::BUCKET_BOUNDS.iter().enumerate() {
            if ms <= bound {
                self.buckets[i].fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
        // Value exceeds all buckets, add to last
        self.buckets[10].fetch_add(1, Ordering::Relaxed);
    }

    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    pub fn sum(&self) -> u64 {
        self.sum.load(Ordering::Relaxed)
    }

    pub fn mean(&self) -> f64 {
        let count = self.count();
        if count == 0 {
            0.0
        } else {
            self.sum() as f64 / count as f64
        }
    }

    /// Returns bucket counts.
    pub fn buckets(&self) -> Vec<(u64, u64)> {
        Self::BUCKET_BOUNDS
            .iter()
            .zip(self.buckets.iter())
            .map(|(&bound, count)| (bound, count.load(Ordering::Relaxed)))
            .collect()
    }
}

/// Collected metrics for the ingestion engine.
#[derive(Debug, Default)]
pub struct Metrics {
    // Ingestion metrics
    pub events_received: Counter,
    pub events_validated: Counter,
    pub events_failed_validation: Counter,
    pub batches_received: Counter,
    pub rate_limited_requests: Counter,

    // Redpanda producer metrics
    pub batches_sent_to_redpanda: Counter,
    pub events_sent_to_redpanda: Counter,
    pub redpanda_send_errors: Counter,

    // Redpanda consumer metrics
    pub events_consumed: Counter,
    pub consumer_errors: Counter,
    pub events_inserted: Counter,

    // ClickHouse metrics
    pub clickhouse_inserts: Counter,
    pub clickhouse_insert_errors: Counter,

    // Latency histograms
    pub ingest_latency_ms: Histogram,
    pub redpanda_latency_ms: Histogram,
    pub clickhouse_latency_ms: Histogram,
    pub batch_insert_latency_ms: Histogram,

    // Gauges
    pub active_connections: Gauge,
    pub queue_depth: Gauge,
    pub backpressure_active: Gauge,
    pub consumer_lag: Gauge,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }
}

/// A snapshot of metrics at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub timestamp: DateTime<Utc>,
    pub events_received: u64,
    pub events_validated: u64,
    pub events_failed_validation: u64,
    pub batches_received: u64,
    pub batches_sent_to_redpanda: u64,
    pub events_sent_to_redpanda: u64,
    pub redpanda_send_errors: u64,
    pub clickhouse_inserts: u64,
    pub clickhouse_insert_errors: u64,
    pub ingest_latency_mean_ms: f64,
    pub redpanda_latency_mean_ms: f64,
    pub clickhouse_latency_mean_ms: f64,
    pub active_connections: u64,
    pub queue_depth: u64,
    pub backpressure_active: bool,
}

impl Metrics {
    /// Takes a snapshot of current metrics.
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp: Utc::now(),
            events_received: self.events_received.get(),
            events_validated: self.events_validated.get(),
            events_failed_validation: self.events_failed_validation.get(),
            batches_received: self.batches_received.get(),
            batches_sent_to_redpanda: self.batches_sent_to_redpanda.get(),
            events_sent_to_redpanda: self.events_sent_to_redpanda.get(),
            redpanda_send_errors: self.redpanda_send_errors.get(),
            clickhouse_inserts: self.clickhouse_inserts.get(),
            clickhouse_insert_errors: self.clickhouse_insert_errors.get(),
            ingest_latency_mean_ms: self.ingest_latency_ms.mean(),
            redpanda_latency_mean_ms: self.redpanda_latency_ms.mean(),
            clickhouse_latency_mean_ms: self.clickhouse_latency_ms.mean(),
            active_connections: self.active_connections.get(),
            queue_depth: self.queue_depth.get(),
            backpressure_active: self.backpressure_active.get() > 0,
        }
    }
}

/// Global metrics registry.
pub static METRICS: std::sync::LazyLock<Metrics> = std::sync::LazyLock::new(Metrics::new);

/// Get the global metrics instance.
pub fn metrics() -> &'static Metrics {
    &METRICS
}
