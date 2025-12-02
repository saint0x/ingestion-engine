//! Background workers for the ingestion engine.
//!
//! Handles async workflows:
//! - Consumer (Redpanda → ClickHouse pipeline)
//! - Compression (free tier 24h → parquet rollup)
//! - Retention (TTL enforcement)
//! - Enrichment (event augmentation)
//! - Backfill (metric recomputation)
//! - Notifications (admin alerts)

pub mod backfill;
pub mod compression;
pub mod consumer;
pub mod enrichment;
pub mod notifications;
pub mod retention;
pub mod scheduler;

pub use consumer::*;
pub use enrichment::EnrichmentWorker;
pub use scheduler::*;
