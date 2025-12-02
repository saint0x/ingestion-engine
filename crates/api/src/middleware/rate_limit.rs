//! Rate limiting middleware.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Token bucket rate limiter.
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
            buckets: Mutex::new(HashMap::new()),
            config,
        }
    }

    /// Check if request is allowed for the given key.
    pub fn check(&self, key: &str) -> bool {
        let mut buckets = self.buckets.lock();

        let bucket = buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket::new(self.config.burst));

        bucket.try_acquire(self.config.rate, self.config.burst)
    }

    /// Clean up stale buckets.
    pub fn cleanup(&self, max_age: Duration) {
        let mut buckets = self.buckets.lock();
        let now = Instant::now();

        buckets.retain(|_, bucket| now.duration_since(bucket.last_update) < max_age);
    }
}

/// Shared rate limiter state.
pub type SharedRateLimiter = Arc<RateLimiter>;
