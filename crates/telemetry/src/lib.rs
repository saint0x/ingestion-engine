//! Internal telemetry for the Overwatch ingestion engine.
//!
//! Instead of external metrics systems, we dogfood our own analytics
//! by sending internal metrics to ClickHouse.

pub mod health;
pub mod metrics;
pub mod tracing_setup;

pub use health::*;
pub use metrics::*;
pub use tracing_setup::*;
