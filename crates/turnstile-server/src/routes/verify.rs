//! `POST /api/v1/verify` — validate a challenge response and issue a JWT.
//!
//! Eight-step verify (order matters — cheapest deterministic checks first,
//! stateful anti-replay last):
//! 1. Recompute & constant-time verify the HMAC over the echoed fields.
//! 2. Confirm the origin is on the allowlist.
//! 3. Reject if the challenge has expired.
//! 4. Atomically mark the challenge spent (`SET NX EX`) — TOCTOU-safe replay
//!    rejection; Redis-down fails CLOSED (503).
//! 5. Re-verify the PoW (seed = hex-decoded `challenge`).
//! 6. Enforce `nonce <= maxnumber`.
//! 7. Issue a short-lived HS256 JWT bound to the origin.

use std::time::Duration;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde::Serialize;
use turnstile_core::pow;
use turnstile_core::protocol::VerifyRequest;
use turnstile_core::rng;

use crate::error::{AppError, AppResult};
use crate::hmac;
use crate::jwt;
use crate::state::AppState;

#[derive(Serialize)]
pub struct VerifyResponse {
    pub success: bool,
    pub token: String,
    pub expires_at: i64,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/verify", post(verify))
}

pub async fn verify(
    State(state): State<AppState>,
    Json(req): Json<VerifyRequest>,
) -> AppResult<Json<VerifyResponse>> {
    let cfg = &state.config;

    // 1. HMAC re-verify (constant-time) over all binding fields.
    let sig_str = hmac::signing_string(
        &req.algorithm,
        &req.salt,
        &req.challenge,
        req.difficulty,
        req.maxnumber,
        req.expires_at,
        &req.origin,
    );
    if !hmac::verify_signature(cfg.hmac_key.as_bytes(), &sig_str, &req.signature) {
        return Err(AppError::VerifyFailed);
    }

    // 2. Origin allowlist.
    if !cfg.is_origin_allowed(&req.origin) {
        return Err(AppError::OriginNotAllowed(req.origin.clone()));
    }

    // 3. Expiry.
    let now = now_secs();
    if req.expires_at <= now {
        return Err(AppError::ChallengeInvalid);
    }

    // 4. Atomic single-use claim (anti-replay). Redis-down -> Err -> 503.
    let claimed = state
        .store
        .claim_spent(&req.challenge, Duration::from_secs(cfg.challenge_ttl_secs))
        .await?;
    if !claimed {
        return Err(AppError::ChallengeAlreadyUsed);
    }

    // 5. PoW re-verify: seed = hex-decoded challenge bytes.
    let seed = hex::decode(&req.challenge)
        .map_err(|_| AppError::BadRequest("malformed challenge hex".into()))?;
    if !pow::verify(&seed, req.nonce, req.difficulty) {
        return Err(AppError::VerifyFailed);
    }

    // 6. Nonce cap.
    if req.nonce > req.maxnumber {
        return Err(AppError::VerifyFailed);
    }

    // 7. Issue JWT (jti = fresh 128-bit random).
    let jti = hex::encode(rng::random_bytes(16).map_err(AppError::internal)?);
    let exp = now + cfg.jwt_ttl_secs as i64;
    let claims = jwt::claims_for(&req.origin, &jti, exp);
    let token = jwt::issue(cfg.jwt_key.as_bytes(), &claims).map_err(AppError::internal)?;

    Ok(Json(VerifyResponse {
        success: true,
        token,
        expires_at: exp,
    }))
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
