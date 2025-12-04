//! Rate limiting middleware.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::warn;

/// Maximum number of rate limit buckets to prevent unbounded memory growth.
/// 10,000 allows for many concurrent API keys while bounding memory.
const MAX_BUCKETS: usize = 10_000;

/// Default max age for stale buckets (1 hour).
const DEFAULT_MAX_AGE: Duration = Duration::from_secs(3600);

/// Token bucket rate limiter with bounded memory.
pub struct RateLimiter {
    buckets: Mutex<HashMap<String, TokenBucket>>,
    config: RateLimitConfig,
}

#[derive(Clone)]
pub struct RateLimitConfig {
    /// Requests per second
    pub rate: u32,
    /// Burst size
    pub burst: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            rate: 10000,
            burst: 50000,
        }
    }
}

struct TokenBucket {
    tokens: f64,
    last_update: Instant,
}

impl TokenBucket {
    fn new(burst: u32) -> Self {
        Self {
            tokens: burst as f64,
            last_update: Instant::now(),
        }
    }

    fn try_acquire(&mut self, rate: u32, burst: u32) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        self.last_update = now;

        // Replenish tokens
        self.tokens = (self.tokens + elapsed * rate as f64).min(burst as f64);

        // Try to consume a token
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            buckets: Mutex::new(HashMap::with_capacity(1000)),
            config,
        }
    }

    /// Check if request is allowed for the given key.
    /// Automatically evicts stale entries when at capacity.
    pub fn check(&self, key: &str) -> bool {
        let mut buckets = self.buckets.lock();

        // If at capacity and key doesn't exist, evict stale entries first
        if buckets.len() >= MAX_BUCKETS && !buckets.contains_key(key) {
            let now = Instant::now();
            let before_count = buckets.len();

            // Remove entries older than DEFAULT_MAX_AGE
            buckets.retain(|_, bucket| now.duration_since(bucket.last_update) < DEFAULT_MAX_AGE);

            let evicted = before_count - buckets.len();
            if evicted > 0 {
                warn!(
                    evicted = evicted,
                    remaining = buckets.len(),
                    "Evicted stale rate limit buckets due to capacity"
                );
            }

            // If still at capacity after cleanup, evict oldest 10%
            if buckets.len() >= MAX_BUCKETS {
                let to_evict = MAX_BUCKETS / 10;
                let mut entries: Vec<_> = buckets
                    .iter()
                    .map(|(k, v)| (k.clone(), v.last_update))
                    .collect();
                entries.sort_by_key(|(_, t)| *t);

                for (key, _) in entries.into_iter().take(to_evict) {
                    buckets.remove(&key);
                }

                warn!(
                    evicted = to_evict,
                    remaining = buckets.len(),
                    "Force-evicted oldest rate limit buckets"
                );
            }
        }

        let bucket = buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket::new(self.config.burst));

        bucket.try_acquire(self.config.rate, self.config.burst)
    }

    /// Clean up stale buckets older than max_age.
    pub fn cleanup(&self, max_age: Duration) {
        let mut buckets = self.buckets.lock();
        let now = Instant::now();
        let before = buckets.len();

        buckets.retain(|_, bucket| now.duration_since(bucket.last_update) < max_age);

        let evicted = before - buckets.len();
        if evicted > 0 {
            tracing::debug!(
                evicted = evicted,
                remaining = buckets.len(),
                "Cleaned up stale rate limit buckets"
            );
        }
    }

    /// Clean up stale buckets using default max age.
    pub fn cleanup_stale(&self) {
        self.cleanup(DEFAULT_MAX_AGE);
    }

    /// Get the number of active buckets.
    pub fn bucket_count(&self) -> usize {
        self.buckets.lock().len()
    }
}

/// Shared rate limiter state.
pub type SharedRateLimiter = Arc<RateLimiter>;
