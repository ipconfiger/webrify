//! Route handlers, grouped by resource. Each sub-module exposes a
//! `router() -> Router<AppState>`; [`router`] composes them.

use axum::Router;

use crate::state::AppState;

pub mod challenge;
pub mod health;
pub mod verify;

/// All API routes, mounted by [`crate::app`] under their final paths.
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(challenge::router())
        .merge(verify::router())
        .merge(health::router())
}
