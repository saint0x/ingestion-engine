//! Core types, schemas, and validation for the Overwatch ingestion engine.

pub mod error;
pub mod events;
pub mod retention;
pub mod schema;
pub mod session;
pub mod tenant;

pub use error::{Error, Result};
pub use events::*;
pub use retention::*;
pub use session::*;
pub use tenant::*;
