//! Route handlers, grouped by resource. Each sub-module exposes a
//! `router() -> Router<AppState>`; [`router`] composes them.

use axum::Router;

use crate::state::AppState;

pub mod challenge;
pub mod demo;
pub mod health;
pub mod metrics;
pub mod verify;
pub mod widget;

/// All routes, mounted by [`crate::app`]. API under `/api/v1`-style paths,
/// widget assets under `/widget/*`, demo page at `/demo`, metrics at `/metrics`.
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(challenge::router())
        .merge(verify::router())
        .merge(health::router())
        .merge(widget::router())
        .merge(demo::router())
        .merge(metrics::router())
}
