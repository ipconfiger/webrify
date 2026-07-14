//! `POST /siteverify` — relying-app backends validate a token here.
//!
//! The widget hands a JWT to the protected site's frontend, which forwards it
//! to ITS backend; that backend calls this endpoint (server-to-server) to check
//! validity **without needing the HMAC signing secret**. Mirrors Cloudflare
//! Turnstile's siteverify model.

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::jwt;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SiteVerifyRequest {
    /// The JWT the widget obtained from `/verify`.
    pub token: String,
    /// Optional expected origin. If set, the token's `aud`/origin must match —
    /// recommended, so a token minted for site A can't be replayed on site B.
    pub origin: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SiteVerifyResponse {
    pub success: bool,
    /// The origin the token was bound to (only on success).
    pub origin: Option<String>,
    /// Token expiry, unix epoch seconds (only on success).
    pub expires_at: Option<i64>,
    /// Present only on failure (human-readable reason).
    pub error: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/siteverify", post(siteverify))
}

pub async fn siteverify(
    State(state): State<AppState>,
    Json(req): Json<SiteVerifyRequest>,
) -> AppResult<Json<SiteVerifyResponse>> {
    let cfg = &state.config;
    // Always 200 — `success` carries the verdict (matches Turnstile siteverify).
    match jwt::verify(cfg.jwt_key.as_bytes(), &req.token, req.origin.as_deref()) {
        Ok(claims) => Ok(Json(SiteVerifyResponse {
            success: true,
            origin: Some(claims.origin),
            expires_at: Some(claims.exp),
            error: None,
        })),
        Err(e) => Ok(Json(SiteVerifyResponse {
            success: false,
            origin: None,
            expires_at: None,
            error: Some(e.to_string()),
        })),
    }
}
