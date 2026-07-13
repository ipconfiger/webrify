//! JWT (HS256) issuance for successful verifications.
//!
//! HS256 is chosen for the single-tenant self-hosted profile: the same party
//! operates the issuing server and validates tokens (via a future
//! `/siteverify` endpoint or a shared secret). An asymmetric EdDSA upgrade is
//! tracked as Phase 4 hardening if relying apps need stateless verification
//! with a publishable public key.

use jsonwebtoken::{encode, EncodingKey, Header};
use turnstile_core::protocol::JwtClaims;

pub type JwtResult<T> = std::result::Result<T, jsonwebtoken::errors::Error>;

/// Build a fresh [`JwtClaims`] for `origin`, valid until `exp` (unix epoch s).
pub fn claims_for(origin: &str, jti: &str, exp: i64) -> JwtClaims {
    JwtClaims {
        iss: "webrify".to_string(),
        aud: origin.to_string(),
        jti: jti.to_string(),
        exp,
        origin: origin.to_string(),
    }
}

/// Sign `claims` into a compact JWT (HS256) using `secret`.
pub fn issue(secret: &[u8], claims: &JwtClaims) -> JwtResult<String> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{decode, DecodingKey, Validation};

    const SECRET: &[u8] = b"jwt-secret";
    const AUD: &str = "https://example.com";

    /// jsonwebtoken 9's default `Validation` enforces `aud`; set the expected one.
    fn validation() -> Validation {
        let mut v = Validation::default();
        v.set_audience(&[AUD]);
        v
    }

    #[test]
    fn issue_then_decode_round_trips() {
        let claims = claims_for(AUD, "token-id-1", 4_102_444_800);
        let token = issue(SECRET, &claims).unwrap();
        let data =
            decode::<JwtClaims>(&token, &DecodingKey::from_secret(SECRET), &validation()).unwrap();
        assert_eq!(data.claims, claims);
    }

    #[test]
    fn decode_rejects_wrong_secret() {
        let claims = claims_for(AUD, "token-id-2", 4_102_444_800);
        let token = issue(SECRET, &claims).unwrap();
        let res = decode::<JwtClaims>(
            &token,
            &DecodingKey::from_secret(b"other-secret"),
            &validation(),
        );
        assert!(res.is_err());
    }

    #[test]
    fn claims_for_binds_origin() {
        let c = claims_for("https://webrify.test", "jti-9", 123);
        assert_eq!(c.iss, "webrify");
        assert_eq!(c.aud, "https://webrify.test");
        assert_eq!(c.jti, "jti-9");
        assert_eq!(c.origin, "https://webrify.test");
        assert_eq!(c.exp, 123);
    }
}
