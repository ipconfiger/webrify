//! Browser fingerprint hashing.
//!
//! The widget collects environment signals (Canvas, WebGL, AudioContext, font
//! enumeration, navigator properties) into a canonical string with sorted keys
//! and stable formatting, then this module hashes it to a stable 128-bit
//! identifier. The resulting hash is:
//!   - bound into the PoW seed (so a solution can't be shared across
//!     fingerprints — each distinct client must do its own work), and
//!   - sent to the server in `VerifyRequest.fingerprint` for risk scoring.
//!
//! Raw signals NEVER leave the client — only the hash (GDPR data minimization).
//! The domain-separation prefix (`webrify-fp-v1:`) keeps this hash distinct
//! from any other SHA-256 use of the same input.

use sha2::{Digest, Sha256};

/// Fingerprint output size: 128 bits (matches [`crate::rng::MIN_CHALLENGE_BYTES`]).
pub const FINGERPRINT_BYTES: usize = 16;

/// Versioned domain-separation prefix. Bumping this invalidates all prior
/// fingerprints (e.g. when the signal set changes).
const PREFIX: &[u8] = b"webrify-fp-v1:";

/// Hash a canonical signal string to a 128-bit fingerprint.
///
/// Deterministic: identical `signals_json` always yields the same fingerprint.
/// The caller MUST build `signals_json` canonically (sorted keys, stable
/// formatting) so the same environment produces the same fingerprint across
/// runs.
pub fn hash(signals_json: &str) -> [u8; FINGERPRINT_BYTES] {
    let mut hasher = Sha256::new();
    hasher.update(PREFIX);
    hasher.update(signals_json.as_bytes());
    let digest = hasher.finalize();
    let mut out = [0u8; FINGERPRINT_BYTES];
    out.copy_from_slice(&digest[..FINGERPRINT_BYTES]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic() {
        assert_eq!(
            hash(r#"{"canvas":"abc","ua":"x"}"#),
            hash(r#"{"canvas":"abc","ua":"x"}"#)
        );
    }

    #[test]
    fn different_signals_yield_different_fingerprints() {
        let a = hash(r#"{"canvas":"abc"}"#);
        let b = hash(r#"{"canvas":"abd"}"#);
        assert_ne!(a, b);
    }

    #[test]
    fn empty_input_is_stable_and_prefix_isolated() {
        // Not the raw SHA-256 of "" — the prefix ensures domain separation.
        let fp = hash("");
        assert_eq!(fp.len(), FINGERPRINT_BYTES);
        let raw = Sha256::new();
        let raw_digest = raw.finalize();
        assert_ne!(&fp[..], &raw_digest[..FINGERPRINT_BYTES]);
    }

    #[test]
    fn output_length() {
        assert_eq!(hash("x").len(), FINGERPRINT_BYTES);
        assert_eq!(hash("x").len() * 8, 128);
    }

    #[test]
    fn key_order_matters_caller_must_canonicalize() {
        // Documented contract: the CALLER canonicalizes. Different byte strings
        // (even semantically-equal JSON with reordered keys) hash differently —
        // this is why the widget must emit sorted keys.
        assert_ne!(hash(r#"{"a":"1","b":"2"}"#), hash(r#"{"b":"2","a":"1"}"#));
    }
}
