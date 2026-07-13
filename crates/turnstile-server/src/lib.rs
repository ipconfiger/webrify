//! Webrify Turnstile verification server (Axum).
//!
//! Self-hosted, single-tenant. The frontend widget is embedded into this
//! binary via `rust-embed` (see [`routes::widget`]); the only external
//! stateful dependency is Redis. Compose the app via [`app`]; `main` boots it.

#![forbid(unsafe_code)]

pub mod config;
pub mod error;
pub mod hmac;
pub mod jwt;
pub mod routes;
pub mod state;
pub mod store;

use axum::http::{HeaderName, HeaderValue, Method};
use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

/// Build the full application router with compression + CORS + tracing layers,
/// ready to serve. Compression (brotli/gzip) applies to `/widget/*` — the
/// embedded JS/WASM text assets compress well.
pub fn app(state: AppState) -> Router {
    let cors = build_cors(&state.config.allowed_origins);
    routes::router()
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(CompressionLayer::new())
}

fn build_cors(allowed: &[String]) -> CorsLayer {
    let origins: Vec<HeaderValue> = allowed.iter().filter_map(|o| o.parse().ok()).collect();
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            HeaderName::from_static("content-type"),
            HeaderName::from_static("origin"),
        ])
}
