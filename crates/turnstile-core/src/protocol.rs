//! Wire protocol types shared between the server and clients (WASM/TS).
//!
//! # Canonical PoW seed
//! The PoW seed passed to [`crate::pow::solve`] / [`crate::pow::verify`] is the
//! raw bytes obtained by **hex-decoding the [`Challenge::challenge`] field**.
//! [`Challenge::salt`] is covered by the HMAC `signature` but is **NOT** part of
//! the PoW seed. Client and server MUST construct the seed identically
//! (hex-decode `challenge`) — any disagreement makes verification fail silently.
//!
//! `ts-rs` derives TypeScript bindings (`.ts` files generated when `cargo test`
//! runs). Numeric fields use `#[ts(type = "number")]` because realistic values
//! fit comfortably in JS's safe-integer range and `JSON.parse` / `JSON.stringify`
//! exchange `number` (ts-rs's default `bigint` mapping for u64/i64 breaks JSON
//! interop on both ends).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A server-minted challenge handed to the client to solve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Challenge {
    /// Hash algorithm, e.g. `"SHA-256"`. Covered by `signature`.
    pub algorithm: String,
    /// Per-challenge random salt (hex). Covered by `signature`. NOT part of the PoW seed.
    pub salt: String,
    /// The PoW seed (hex). Hex-decode these bytes to feed [`crate::pow::solve`].
    pub challenge: String,
    /// Required leading zero bits in the PoW hash.
    pub difficulty: u32,
    /// Upper bound on the nonce search space. Covered by `signature`.
    #[ts(type = "number")]
    pub maxnumber: u64,
    /// Challenge expiry, unix epoch seconds. Covered by `signature`.
    #[ts(type = "number")]
    pub expires_at: i64,
    /// Origin this challenge is bound to. Covered by `signature`.
    pub origin: String,
    /// `HMAC-SHA256(secret, algorithm|salt|challenge|difficulty|maxnumber|expires_at|origin)` (hex).
    pub signature: String,
}

/// A client's verification submission.
///
/// Carries every field the server needs to recompute the HMAC statelessly
/// (defense-in-depth) and to re-verify the PoW. The server STILL consults its
/// pending-challenge cache to enforce single-use (replay protection via
/// `SET NX EX`); the echoed `signature` is re-verified independently so a
/// tampered field is rejected even if the cache lookup is bypassed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VerifyRequest {
    /// Hash algorithm (echoed from the challenge; needed to recompute HMAC).
    pub algorithm: String,
    /// The PoW seed (hex; hex-decode identically to the client).
    pub challenge: String,
    /// Per-challenge salt (hex; echoed).
    pub salt: String,
    /// Required leading zero bits (echoed).
    pub difficulty: u32,
    /// Nonce search-space cap (echoed); server checks `nonce <= maxnumber`.
    #[ts(type = "number")]
    pub maxnumber: u64,
    /// Expiry (echoed).
    #[ts(type = "number")]
    pub expires_at: i64,
    /// Origin (echoed).
    pub origin: String,
    /// HMAC signature (echoed).
    pub signature: String,
    /// The nonce the client found.
    #[ts(type = "number")]
    pub nonce: u64,
    /// Client-generated key making repeated submits idempotent.
    pub idempotency_key: String,
    /// Optional client-computed fingerprint hash (default TS mapping: `string | null`).
    pub fingerprint: Option<String>,
    /// Optional client-computed behavior score in `[0.0, 1.0]` (higher = more human-like).
    pub behavior_score: Option<f32>,
}

/// JWT body issued by the server after a successful verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct JwtClaims {
    /// Issuer (`"webrify"`).
    pub iss: String,
    /// Audience (the bound origin).
    pub aud: String,
    /// Unique token id (random, for revocation/audit).
    pub jti: String,
    /// Expiry, unix epoch seconds.
    #[ts(type = "number")]
    pub exp: i64,
    /// Origin this token is bound to.
    pub origin: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_challenge() -> Challenge {
        Challenge {
            algorithm: "SHA-256".into(),
            salt: "deadbeef".into(),
            challenge: "cafebabe1234".into(),
            difficulty: 14,
            maxnumber: 100_000,
            expires_at: 1_758_000_000,
            origin: "https://example.com".into(),
            signature: "aabbccdd".into(),
        }
    }

    #[test]
    fn challenge_serde_round_trip() {
        let challenge = sample_challenge();
        let json = serde_json::to_string(&challenge).unwrap();
        let back: Challenge = serde_json::from_str(&json).unwrap();
        assert_eq!(challenge, back);
    }

    fn sample_verify_request() -> VerifyRequest {
        VerifyRequest {
            algorithm: "SHA-256".into(),
            challenge: "cafebabe1234".into(),
            salt: "deadbeef".into(),
            difficulty: 14,
            maxnumber: 100_000,
            expires_at: 1_758_000_000,
            origin: "https://example.com".into(),
            signature: "aabbccdd".into(),
            nonce: 4721,
            idempotency_key: "550e8400-e29b-41d4-a716-446655440000".into(),
            fingerprint: Some("fp-hash".into()),
            behavior_score: Some(0.82),
        }
    }

    #[test]
    fn verify_request_serde_round_trip_with_optionals() {
        let req = sample_verify_request();
        let json = serde_json::to_string(&req).unwrap();
        let back: VerifyRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn verify_request_serde_round_trip_without_optionals() {
        let req = VerifyRequest {
            fingerprint: None,
            behavior_score: None,
            ..sample_verify_request()
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: VerifyRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn jwt_claims_serde_round_trip() {
        let claims = JwtClaims {
            iss: "webrify".into(),
            aud: "https://example.com".into(),
            jti: "token-id".into(),
            exp: 1_758_000_900,
            origin: "https://example.com".into(),
        };
        let json = serde_json::to_string(&claims).unwrap();
        let back: JwtClaims = serde_json::from_str(&json).unwrap();
        assert_eq!(claims, back);
    }
}
