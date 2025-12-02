//! HTTP API layer for the ingestion engine.

pub mod extractors;
pub mod middleware;
pub mod response;
pub mod routes;
pub mod state;

pub use routes::router;
pub use state::AppState;
