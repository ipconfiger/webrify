//! `GET /health` (liveness) and `GET /ready` (readiness, probes Redis).
//!
//! `/ready` fails CLOSED: when Redis is unreachable it returns 503 (via the
//! `?`-propagated `AppError::Redis`), so a load balancer pulls the instance
//! rather than routing verification traffic it can't honour.

use axum::extract::State;
use axum::routing::get;
use axum::Router;

use crate::error::AppResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
}

/// Liveness: the process is up.
pub async fn health() -> AppResult<&'static str> {
    Ok("ok")
}

/// Readiness: Redis is reachable; otherwise 503 (fail-closed).
pub async fn ready(State(state): State<AppState>) -> AppResult<&'static str> {
    state.store.ping().await?;
    Ok("ready")
}
