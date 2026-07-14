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
/// Builds the canonical seed as `hex_decode(challenge_hex) || hex_decode(fingerprint_hex)`
/// (the fingerprint makes the solution non-transferable across clients; pass
/// `None` for the PoW-only / GDPR fallback path), then searches `[0, maxnumber]`
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
    fingerprint_hex: Option<String>,
    difficulty: u32,
    maxnumber: f64,
) -> Result<f64, JsValue> {
    let mut seed = hex::decode(challenge_hex).map_err(js_err)?;
    if let Some(fp_hex) = fingerprint_hex {
        let fp = hex::decode(&fp_hex).map_err(js_err)?;
        seed.extend_from_slice(&fp);
    }
    let cap = maxnumber as u64;
    let nonce = turnstile_core::pow::solve_bounded(&seed, difficulty, cap)
        .ok_or_else(|| JsValue::from_str("no PoW solution within maxnumber"))?;
    Ok(nonce as f64)
}

/// Compute the 128-bit browser fingerprint (lowercase hex) of a canonical
/// signal string.
///
/// The widget collects environment signals (Canvas/WebGL/Audio/fonts/navigator)
/// into a canonical JSON string with sorted keys and passes it here. The
/// returned 16-byte hash is bound into the PoW seed and sent to the server for
/// risk scoring; raw signals never leave the client (GDPR minimization).
#[wasm_bindgen]
pub fn fingerprint_hash(signals_json: &str) -> String {
    hex::encode(turnstile_core::fingerprint::hash(signals_json))
}

/// Human-likeness behavior score in `[0, 100]` (higher = more human), or `null`
/// if there's too little interaction signal to judge.
///
/// `mouse` is a flat array of `[x, y, t_ms]` triples; `click_intervals_ms` and
/// `key_intervals_ms` are arrays of inter-event intervals in ms;
/// `click_positions` is a flat array of `[x, y]` pairs. Any may be
/// empty — the scorer degrades gracefully. Computed in WASM so the canonical
/// logic lives in one place (`turnstile_core::behavior`).
#[wasm_bindgen]
pub fn behavior_score(
    mouse: &js_sys::Float64Array,
    click_intervals_ms: &js_sys::Float64Array,
    key_intervals_ms: &js_sys::Float64Array,
    click_positions: &js_sys::Float64Array,
) -> Option<u32> {
    use turnstile_core::behavior::{BehaviorInput, MouseSample};
    let mouse: Vec<MouseSample> = mouse
        .to_vec()
        .chunks_exact(3)
        .map(|c| MouseSample {
            x: c[0],
            y: c[1],
            t_ms: c[2],
        })
        .collect();
    let click_positions: Vec<(f64, f64)> = click_positions
        .to_vec()
        .chunks_exact(2)
        .map(|c| (c[0], c[1]))
        .collect();
    turnstile_core::behavior::score(&BehaviorInput {
        mouse,
        click_intervals_ms: click_intervals_ms.to_vec(),
        key_intervals_ms: key_intervals_ms.to_vec(),
        click_positions,
    })
}

fn js_err<E: std::fmt::Display>(e: E) -> JsValue {
    JsValue::from_str(&e.to_string())
}
