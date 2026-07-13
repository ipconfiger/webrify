//! `POST /api/v1/challenge` — mint an HMAC-signed challenge.
//!
//! Validates the request `Origin` against the allowlist BEFORE signing, so the
//! signature can only ever bind an allowed origin (defense at issuance, not just
//! at verify).

use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use turnstile_core::protocol::Challenge;
use turnstile_core::rng;

use crate::error::{AppError, AppResult};
use crate::hmac;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/challenge", post(create))
}

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Challenge>> {
    let origin = origin_header(&headers)?.to_string();
    if !state.config.is_origin_allowed(&origin) {
        return Err(AppError::OriginNotAllowed(origin));
    }

    let cfg = &state.config;
    let salt = hex::encode(rng::random_bytes(16).map_err(AppError::internal)?);
    let seed = rng::challenge_seed().map_err(AppError::internal)?;
    let challenge_hex = hex::encode(seed);
    let now = now_secs();
    let expires_at = now + cfg.challenge_ttl_secs as i64;

    let sig_str = hmac::signing_string(
        "SHA-256",
        &salt,
        &challenge_hex,
        cfg.difficulty,
        cfg.maxnumber,
        expires_at,
        &origin,
    );
    let signature = hmac::sign(cfg.hmac_key.as_bytes(), &sig_str);

    Ok(Json(Challenge {
        algorithm: "SHA-256".to_string(),
        salt,
        challenge: challenge_hex,
        difficulty: cfg.difficulty,
        maxnumber: cfg.maxnumber,
        expires_at,
        origin,
        signature,
    }))
}

fn origin_header(headers: &HeaderMap) -> AppResult<&str> {
    headers
        .get(axum::http::header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::BadRequest("missing Origin header".into()))
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
