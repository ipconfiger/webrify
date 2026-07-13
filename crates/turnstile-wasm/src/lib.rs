//! Webrify Turnstile WASM bindings.
//!
//! Thin [`wasm_bindgen`] layer over [`turnstile_core`]. The widget's Web Worker
//! loads this module and calls [`solve_challenge`] off the main thread so the
//! tight SHA-256 PoW loop never freezes the UI.
//!
//! Built for `wasm32-unknown-unknown` via `wasm-pack` (`--target web`); this
//! crate is NOT part of the default `cargo build` (excluded via the workspace
//! `default-members` setting).

#![forbid(unsafe_code)]

use wasm_bindgen::prelude::*;

/// Module init: route Rust panics to `console.error` for usable browser debugging.
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Solve the PoW.
///
/// Hex-decodes `challenge_hex` to the seed bytes, then searches `[0, maxnumber]`
/// for the smallest nonce such that `SHA-256(seed || nonce_be)` has `difficulty`
/// leading zero bits.
///
/// `maxnumber` is taken as `f64` (and the nonce returned as `f64`) to avoid
/// BigInt/JSON friction at the JS boundary — realistic nonces are far below
/// 2^53 so the casts are exact. Throws if the hex is malformed or no solution
/// exists within the cap.
#[wasm_bindgen]
pub fn solve_challenge(
    challenge_hex: &str,
    difficulty: u32,
    maxnumber: f64,
) -> Result<f64, JsValue> {
    let seed = hex::decode(challenge_hex).map_err(js_err)?;
    let cap = maxnumber as u64;
    let nonce = turnstile_core::pow::solve_bounded(&seed, difficulty, cap)
        .ok_or_else(|| JsValue::from_str("no PoW solution within maxnumber"))?;
    Ok(nonce as f64)
}

fn js_err<E: std::fmt::Display>(e: E) -> JsValue {
    JsValue::from_str(&e.to_string())
}
