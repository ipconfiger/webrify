//! Cryptographic randomness for challenges, salts, and tokens.
//!
//! Backed by the OS CSPRNG via [`rand::rngs::OsRng`]. `OsRng` in rand 0.9
//! implements only the fallible [`rand_core::TryRngCore`] (OS entropy can
//! fail), so every primitive here returns [`Result`] and callers MUST fail
//! closed (refuse to mint challenges / tokens) on error rather than panic.
//!
//! On `wasm32-unknown-unknown` this routes through getrandom's `wasm_js`
//! backend (the `wasm_js` feature is enabled for the wasm target in
//! `Cargo.toml`, which feature-unification propagates to `rand`'s getrandom
//! dep).

use rand_core::TryRngCore;

/// Minimum entropy for a challenge seed: 128 bits (16 bytes).
pub const MIN_CHALLENGE_BYTES: usize = 16;

/// Error from the underlying OS CSPRNG (rand_core 0.9 models this as the
/// associated `TryRngCore::Error` of `OsRng`, so we name it via the trait
/// rather than guessing the concrete type).
pub type RngError = <rand::rngs::OsRng as TryRngCore>::Error;

/// Fill `dest` with cryptographically secure random bytes from the OS RNG.
///
/// Works on native (getrandom OS entropy) and `wasm32-unknown-unknown`
/// (getrandom `wasm_js` -> `crypto.getRandomValues`). Returns an error if the
/// OS entropy source fails; callers SHOULD fail closed.
pub fn fill_random(dest: &mut [u8]) -> Result<(), RngError> {
    rand::rngs::OsRng.try_fill_bytes(dest)
}

/// Allocate `len` cryptographically random bytes.
pub fn random_bytes(len: usize) -> Result<Vec<u8>, RngError> {
    let mut buf = vec![0u8; len];
    fill_random(&mut buf)?;
    Ok(buf)
}

/// Generate a fresh 128-bit challenge seed.
pub fn challenge_seed() -> Result<[u8; MIN_CHALLENGE_BYTES], RngError> {
    let mut seed = [0u8; MIN_CHALLENGE_BYTES];
    fill_random(&mut seed)?;
    Ok(seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_random_is_nontrivial() {
        // A 32-byte CSPRNG output is effectively never all-zero.
        let mut buf = [0u8; 32];
        fill_random(&mut buf).unwrap();
        assert!(buf.iter().any(|&b| b != 0));
    }

    #[test]
    fn random_bytes_respects_requested_length() {
        assert_eq!(random_bytes(0).unwrap().len(), 0);
        assert_eq!(random_bytes(16).unwrap().len(), 16);
        assert_eq!(random_bytes(64).unwrap().len(), 64);
    }

    #[test]
    fn challenge_seed_is_128_bits() {
        let seed = challenge_seed().unwrap();
        assert_eq!(seed.len(), MIN_CHALLENGE_BYTES);
        assert_eq!(seed.len() * 8, 128);
    }

    #[test]
    fn successive_challenge_seeds_differ() {
        // Collision probability for 128-bit CSPRNG output is ~2^-128.
        assert_ne!(challenge_seed().unwrap(), challenge_seed().unwrap());
    }
}
