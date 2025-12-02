//! Memory and size limits for the ingestion engine.
//!
//! MEMORY SAFETY: These limits prevent DoS attacks via memory exhaustion.
//! Limits are tuned for 100k events/sec throughput target.
//!
//! # Design Rationale
//!
//! At 100k events/sec with 1KB average event size:
//! - Sustained throughput: 100MB/sec
//! - 1MB batch limit → ~1000 events/batch → 100 batches/sec
//! - Memory per batch: 1MB × concurrent_requests = predictable working set
//!
//! # Usage Note
//!
//! Constants used at runtime: `MAX_BATCH_SIZE_BYTES`, `MAX_EVENT_SIZE_BYTES`,
//! `MAX_CUSTOM_PROPERTIES_BYTES`, `MAX_FUTURE_SKEW_SECS`, `MAX_EVENT_AGE_HOURS`.
//!
//! Other constants are exported for downstream crates and documentation.
//! The `#[validate]` derive macro requires literal values in attributes,
//! so field limits are duplicated there. Keep both in sync when modifying.

// === Batch Limits ===

/// Maximum batch payload size in bytes (1MB).
///
/// Prevents memory spikes from oversized requests.
/// At 1000 events per batch with 1KB avg, this is the expected max.
pub const MAX_BATCH_SIZE_BYTES: usize = 1024 * 1024;

/// Maximum events per batch.
///
/// Already enforced via validator, centralized here for reference.
pub const MAX_BATCH_EVENTS: usize = 1000;

/// Maximum single event size in bytes (32KB).
///
/// Accounts for custom events with large properties payloads.
pub const MAX_EVENT_SIZE_BYTES: usize = 32 * 1024;

// === Custom Event Limits ===

/// Maximum custom properties JSON size in bytes (16KB).
///
/// Balances flexibility (Amplitude allows 32KB) with memory safety.
/// Most real-world custom events are under 1KB.
pub const MAX_CUSTOM_PROPERTIES_BYTES: usize = 16 * 1024;

// === String Field Limits (chars) ===

/// User agent string max length.
/// Browser UAs: 100-300 typical, 500+ with extensions.
pub const MAX_USER_AGENT_LEN: usize = 512;

/// IP address max length (IPv6 = 45 chars).
pub const MAX_IP_LEN: usize = 45;

/// Timezone identifier max length.
/// IANA names like "America/Los_Angeles" are ~25 chars.
pub const MAX_TIMEZONE_LEN: usize = 64;

/// Language tag max length.
/// BCP 47 tags like "en-US" are ~5 chars.
pub const MAX_LANGUAGE_LEN: usize = 16;

/// User ID max length.
/// UUIDs=36, emails=~50, custom IDs up to 128.
pub const MAX_USER_ID_LEN: usize = 128;

/// Referrer URL max length.
/// Matches HTTP Referer header limit.
pub const MAX_REFERRER_LEN: usize = 2048;

/// HTML element tag name max length.
/// Tags are short (div, button, custom-element).
pub const MAX_ELEMENT_TAG_LEN: usize = 64;

/// Scroll element CSS selector max length.
pub const MAX_ELEMENT_SELECTOR_LEN: usize = 256;

/// Tenant display name max length.
pub const MAX_TENANT_NAME_LEN: usize = 200;

// === Performance Metric Bounds ===

/// LCP max in milliseconds (60 seconds).
pub const MAX_LCP_MS: f64 = 60_000.0;

/// FID max in milliseconds (10 seconds).
pub const MAX_FID_MS: f64 = 10_000.0;

/// CLS max value (Google considers >0.25 poor, 10 is extreme).
pub const MAX_CLS: f64 = 10.0;

/// TTFB max in milliseconds (60 seconds).
pub const MAX_TTFB_MS: f64 = 60_000.0;

/// DOM Content Loaded max in milliseconds (2 minutes).
pub const MAX_DOM_LOADED_MS: f64 = 120_000.0;

/// Load Complete max in milliseconds (5 minutes).
pub const MAX_LOAD_COMPLETE_MS: f64 = 300_000.0;

/// Resource count max (heavy pages have 500+, 10k is extreme).
pub const MAX_RESOURCE_COUNT: u32 = 10_000;

// === Timestamp Bounds ===

/// Maximum allowed clock skew for future timestamps (seconds).
pub const MAX_FUTURE_SKEW_SECS: i64 = 5;

/// Maximum age for stale events (hours).
pub const MAX_EVENT_AGE_HOURS: i64 = 24;
