//! Server error type. Maps cleanly to HTTP responses via [`IntoResponse`].
//!
//! Internal errors NEVER leak details to the client — they are logged via
//! `tracing` and projected as a generic `500 INTERNAL` / `503 UNAVAILABLE`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

pub type AppResult<T> = std::result::Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("origin {0:?} is not allowed")]
    OriginNotAllowed(String),

    #[error("challenge expired or not found")]
    ChallengeInvalid,

    #[error("challenge already used (replay rejected)")]
    ChallengeAlreadyUsed,

    #[error("verification failed")]
    VerifyFailed,

    /// Redis error — fail-closed: the verification system MUST refuse to
    /// operate when its anti-replay store is unavailable.
    #[error(transparent)]
    Redis(#[from] redis::RedisError),

    #[error("internal error")]
    Internal,
}

impl AppError {
    /// Wrap an unexpected internal error. The concrete value is logged; the
    /// client only sees the generic [`AppError::Internal`] projection.
    pub fn internal<E: std::fmt::Debug>(e: E) -> Self {
        tracing::error!(error = ?e, "internal error");
        AppError::Internal
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            AppError::OriginNotAllowed(o) => (
                StatusCode::FORBIDDEN,
                format!("origin {o:?} is not allowed"),
            ),
            AppError::ChallengeInvalid => (
                StatusCode::BAD_REQUEST,
                "challenge expired or not found".into(),
            ),
            AppError::ChallengeAlreadyUsed => {
                (StatusCode::CONFLICT, "challenge already used".into())
            }
            AppError::VerifyFailed => (StatusCode::FORBIDDEN, "verification failed".into()),
            // Fail-closed: surface 503 for any Redis error (anti-replay store down).
            AppError::Redis(e) => {
                tracing::error!(error = ?e, "redis unavailable; failing closed");
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "verification temporarily unavailable".into(),
                )
            }
            AppError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into()),
        };
        (status, Json(ErrorBody { error: msg })).into_response()
    }
}
