//! Hashcash-style Proof-of-Work over SHA-256.
//!
//! The server mints a challenge `seed`; the client must find a `nonce` such
//! that `SHA-256(seed || nonce_be)` has at least `difficulty` leading zero
//! *bits*. [`solve`] searches from nonce 0 upward and returns the smallest
//! solution; [`verify`] cheaply re-checks a claim. Both are pure functions
//! with no I/O, so they build identically for native and wasm32 and are fully
//! deterministic for a given `(seed, difficulty)`.

use sha2::{Digest, Sha256};

/// Size of a SHA-256 digest in bytes.
const DIGEST_LEN: usize = 32;

/// Hash `seed` concatenated with `nonce` laid out big-endian over 8 bytes.
fn hash_nonce(seed: &[u8], nonce: u64) -> [u8; DIGEST_LEN] {
    let mut hasher = Sha256::new();
    hasher.update(seed);
    hasher.update(nonce.to_be_bytes());
    let result = hasher.finalize();
    let mut out = [0u8; DIGEST_LEN];
    out.copy_from_slice(&result);
    out
}

/// Count the leading zero bits in `digest`.
pub fn leading_zero_bits(digest: &[u8]) -> u32 {
    let mut count = 0u32;
    for &byte in digest {
        if byte == 0 {
            count += 8;
        } else {
            count += byte.leading_zeros();
            break;
        }
    }
    count
}

/// Verify that `nonce` satisfies the PoW for `seed` at `difficulty`
/// (leading zero bits). `difficulty == 0` is always satisfied.
pub fn verify(seed: &[u8], nonce: u64, difficulty: u32) -> bool {
    if difficulty == 0 {
        return true;
    }
    let digest = hash_nonce(seed, nonce);
    leading_zero_bits(&digest) >= difficulty
}

/// Find the smallest `nonce` such that [`verify`] holds. Deterministic for a
/// given `(seed, difficulty)`.
///
/// **Seed contract**: `seed` is the opaque PoW seed. In the Webrify protocol
/// the seed is the hex-decoded `Challenge.challenge` field — see
/// [`crate::protocol::Challenge`]. Server-side verification MUST call [`verify`]
/// (or rely on the same `hash_nonce` encoding below) rather than reimplementing
/// the hash, so the nonce encoding stays identical across native and WASM.
///
/// # Panics
/// Panics if the `u64` nonce space is exhausted (astronomically unlikely for
/// any sane difficulty; use [`solve_bounded`] in constrained clients).
pub fn solve(seed: &[u8], difficulty: u32) -> u64 {
    if difficulty == 0 {
        return 0;
    }
    let mut nonce = 0u64;
    loop {
        if verify(seed, nonce, difficulty) {
            return nonce;
        }
        nonce = nonce
            .checked_add(1)
            .expect("PoW nonce space exhausted; reduce difficulty or raise maxnumber");
    }
}

/// Like [`solve_bounded`], but searches `[start, end]` instead of `[0, max]`.
/// Used by multi-threaded clients to split the search space across workers.
pub fn solve_bounded_range(seed: &[u8], difficulty: u32, start: u64, end: u64) -> Option<u64> {
    if difficulty == 0 {
        return Some(0);
    }
    (start..=end).find(|&nonce| verify(seed, nonce, difficulty))
}

/// Like [`solve`], but gives up if no valid nonce is found within `[0, maxnumber]`.
/// Returns `Some(nonce)` on success, `None` if the bounded search space is
/// exhausted. Use this in constrained clients (e.g. the WASM widget) to respect
/// the server's `maxnumber` cap and avoid hanging the tab on misconfigured
/// difficulty.
pub fn solve_bounded(seed: &[u8], difficulty: u32, maxnumber: u64) -> Option<u64> {
    if difficulty == 0 {
        return Some(0);
    }
    (0..=maxnumber).find(|&nonce| verify(seed, nonce, difficulty))
}

/// Maximum additional difficulty bits from risk escalation (Phase 3c adaptive).
const MAX_ESCALATION_BITS: u32 = 6;
/// Absolute difficulty cap so a flagged client can't be locked out indefinitely.
const DIFFICULTY_CAP: u32 = 24;

/// Compute the effective difficulty, escalating by up to [`MAX_ESCALATION_BITS`]
/// based on the client's recent risk-escalation count, capped at
/// [`DIFFICULTY_CAP`]. Each bit roughly doubles the PoW cost, so repeat
/// offenders pay exponentially more without ever being fully locked out.
pub fn adjust_difficulty(base: u32, escalations: u32) -> u32 {
    (base + escalations.min(MAX_ESCALATION_BITS)).min(DIFFICULTY_CAP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_zero_bits_known_values() {
        assert_eq!(leading_zero_bits(&[]), 0);
        assert_eq!(leading_zero_bits(&[0xFF]), 0);
        assert_eq!(leading_zero_bits(&[0x7F]), 1);
        assert_eq!(leading_zero_bits(&[0x0F]), 4);
        assert_eq!(leading_zero_bits(&[0x01]), 7);
        assert_eq!(leading_zero_bits(&[0x00]), 8);
        assert_eq!(leading_zero_bits(&[0x00, 0xFF]), 8);
        assert_eq!(leading_zero_bits(&[0x00, 0x0F]), 12);
        assert_eq!(leading_zero_bits(&[0x00, 0x00, 0x01]), 23);
        assert_eq!(leading_zero_bits(&[0xFF; 32]), 0);
        assert_eq!(leading_zero_bits(&[0x00; 32]), 256);
    }

    #[test]
    fn difficulty_zero_always_satisfies() {
        let seed = b"any-seed";
        assert!(verify(seed, 0, 0));
        assert!(verify(seed, 42, 0));
        assert!(verify(seed, u64::MAX, 0));
    }

    #[test]
    fn adjust_difficulty_escalates_then_caps() {
        assert_eq!(adjust_difficulty(14, 0), 14);
        assert_eq!(adjust_difficulty(14, 3), 17);
        // Bump is capped at MAX_ESCALATION_BITS (6).
        assert_eq!(adjust_difficulty(14, 100), 20);
        // And the total is capped at DIFFICULTY_CAP (24).
        assert_eq!(adjust_difficulty(20, 10), 24);
        assert_eq!(adjust_difficulty(30, 0), 24);
    }

    #[test]
    fn solve_returns_smallest_valid_nonce() {
        let seed = b"wave-a-pow-test-seed";
        let difficulty = 8; // ~256 iterations on average
        let nonce = solve(seed, difficulty);
        // The returned nonce must satisfy the difficulty.
        assert!(verify(seed, nonce, difficulty));
        // And every strictly smaller nonce must NOT (smallest-wins invariant).
        for n in 0..nonce {
            assert!(
                !verify(seed, n, difficulty),
                "smaller nonce {n} also satisfies"
            );
        }
    }

    #[test]
    fn solve_is_deterministic() {
        let seed = b"determinism-check";
        let difficulty = 12;
        assert_eq!(solve(seed, difficulty), solve(seed, difficulty));
    }

    #[test]
    fn verify_rejects_just_below_threshold() {
        // Deterministic reject: solve at difficulty 8, then demand one more
        // leading-zero bit than the resulting digest actually has.
        let seed = b"reject-threshold-seed";
        let nonce = solve(seed, 8);
        let actual = leading_zero_bits(&hash_nonce(seed, nonce));
        assert!(actual >= 8);
        assert!(!verify(seed, nonce, actual + 1));
    }

    #[test]
    fn round_trip_across_seeds_and_difficulties() {
        for (seed, diff) in [
            (&b"s1"[..], 4u32),
            (&b"s2"[..], 8),
            (&b"s3"[..], 10),
            (&b"longer-seed-value"[..], 6),
        ] {
            let nonce = solve(seed, diff);
            assert!(
                verify(seed, nonce, diff),
                "round trip failed for {seed:?} @ {diff}"
            );
        }
    }

    #[test]
    fn solve_bounded_difficulty_zero() {
        assert_eq!(solve_bounded(b"any", 0, 5), Some(0));
    }

    #[test]
    fn solve_bounded_boundary_around_full_solution() {
        // Deterministic: use the unbounded solution as the exact boundary.
        let seed = b"bounded-boundary-seed";
        let difficulty = 8;
        let full = solve(seed, difficulty);
        // Cap just below the smallest solution -> None.
        if full > 0 {
            assert_eq!(solve_bounded(seed, difficulty, full - 1), None);
        }
        // Cap exactly at the solution -> Some(full).
        assert_eq!(solve_bounded(seed, difficulty, full), Some(full));
        // Cap above the solution -> still the smallest valid nonce.
        assert_eq!(solve_bounded(seed, difficulty, full + 10), Some(full));
    }

    #[test]
    fn solve_bounded_returns_none_when_space_too_small() {
        // Difficulty 20 expects ~1M iterations; a cap of 5 almost certainly misses.
        let result = solve_bounded(b"tiny-cap-seed", 20, 5);
        assert!(result.is_none(), "expected no solution within tiny cap");
    }
}
