//! Route handlers, grouped by resource. Each sub-module exposes a
//! `router() -> Router<AppState>`; [`router`] composes them.

use axum::Router;

use crate::state::AppState;

pub mod challenge;
pub mod demo;
pub mod health;
pub mod verify;
pub mod widget;

/// All routes, mounted by [`crate::app`]. API under `/api/v1`-style paths,
/// widget assets under `/widget/*`, demo page at `/demo`.
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(challenge::router())
        .merge(verify::router())
        .merge(health::router())
        .merge(widget::router())
        .merge(demo::router())
}
