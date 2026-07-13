//! Webrify Turnstile WASM bindings (scaffold).
//!
//! `#[wasm_bindgen]` exports for the client verification engine land in
//! Phase 1.11. This crate compiles as `cdylib` + `rlib`; it is built for
//! `wasm32-unknown-unknown` via `wasm-pack` (NOT in the default `cargo build`).

#![forbid(unsafe_code)]
