//! HMAC-SHA256 challenge binding.
//!
//! The challenge signature covers every binding parameter so any client
//! tampering (e.g. lowering `difficulty`) is detected at verify time.
//! Re-verification uses [`hmac::Mac::verify_slice`], which compares in
//! constant time — preventing a timing oracle on signature checking.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Build the canonical signing string. Field order is part of the protocol
/// contract and MUST be identical at sign and verify time.
pub fn signing_string(
    algorithm: &str,
    salt: &str,
    challenge: &str,
    difficulty: u32,
    maxnumber: u64,
    expires_at: i64,
    origin: &str,
) -> String {
    format!("{algorithm}|{salt}|{challenge}|{difficulty}|{maxnumber}|{expires_at}|{origin}")
}

/// Compute the HMAC-SHA256 signature (lowercase hex) over `signing_string`.
///
/// `secret` is the server's HMAC key (validated non-empty at startup). HMAC
/// accepts any key length per RFC 2104, so key construction is infallible.
#[allow(clippy::missing_const_for_fn)]
pub fn sign(secret: &[u8], signing_string: &str) -> String {
    // RFC 2104: HMAC key length is unbounded; `new_from_slice` cannot fail here.
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC-SHA256 accepts any key length");
    mac.update(signing_string.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Constant-time verification that `signature_hex` matches the HMAC of
/// `signing_string` under `secret`. Returns `false` on any mismatch, on
/// malformed hex, or on key errors (treated as a failed verification, never
/// panicked — the input is untrusted).
pub fn verify_signature(secret: &[u8], signing_string: &str, signature_hex: &str) -> bool {
    let expected = match hex::decode(signature_hex) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let mut mac = match HmacSha256::new_from_slice(secret) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(signing_string.as_bytes());
    // verify_slice is constant-time (uses CTimeEq internally).
    mac.verify_slice(&expected).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"super-secret-hmac-key";

    fn ss(challenge: &str, difficulty: u32, origin: &str) -> String {
        signing_string(
            "SHA-256",
            "deadbeef",
            challenge,
            difficulty,
            100_000,
            1_758_000_000,
            origin,
        )
    }

    #[test]
    fn sign_then_verify_round_trips() {
        let s = ss("cafebabe", 14, "https://example.com");
        let sig = sign(SECRET, &s);
        assert!(verify_signature(SECRET, &s, &sig));
    }

    #[test]
    fn verify_rejects_tampered_difficulty() {
        let legit = ss("cafebabe", 14, "https://example.com");
        let sig = sign(SECRET, &legit);
        // Client claims difficulty 4 (easier) — signing string differs.
        let tampered = ss("cafebabe", 4, "https://example.com");
        assert!(!verify_signature(SECRET, &tampered, &sig));
    }

    #[test]
    fn verify_rejects_tampered_origin() {
        let sig = sign(SECRET, &ss("cafebabe", 14, "https://example.com"));
        let forged = ss("cafebabe", 14, "https://evil.com");
        assert!(!verify_signature(SECRET, &forged, &sig));
    }

    #[test]
    fn verify_rejects_wrong_secret() {
        let s = ss("cafebabe", 14, "https://example.com");
        let sig = sign(SECRET, &s);
        assert!(!verify_signature(b"different-secret", &s, &sig));
    }

    #[test]
    fn verify_rejects_malformed_hex() {
        let s = ss("cafebabe", 14, "https://example.com");
        assert!(!verify_signature(SECRET, &s, "not-hex!"));
    }

    #[test]
    fn signing_string_is_canonical() {
        let s = signing_string("SHA-256", "aa", "cc", 8, 1000, 99, "https://x");
        assert_eq!(s, "SHA-256|aa|cc|8|1000|99|https://x");
    }
}
