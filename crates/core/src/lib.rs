//! Core types, schemas, and validation for the Overwatch ingestion engine.

pub mod auth;
pub mod error;
pub mod events;
pub mod limits;
pub mod retention;
pub mod schema;
pub mod sdk_event;
pub mod session;
pub mod tenant;

pub use auth::*;
pub use error::{
    AuthErrorCode, DbErrorCode, Error, RateLimitErrorCode, Result, ValidationErrorCode,
};
pub use events::*;
pub use retention::*;
pub use sdk_event::*;
pub use session::*;
pub use tenant::*;
