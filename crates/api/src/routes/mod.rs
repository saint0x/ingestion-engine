//! API routes.

pub mod health;
pub mod ingest;

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use crate::state::AppState;

/// Creates the API router.
pub fn router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/overwatch-ingest", post(ingest::ingest_handler))
        .route("/health", get(health::health_handler))
        .route("/health/ready", get(health::ready_handler))
        .route("/health/live", get(health::live_handler))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
