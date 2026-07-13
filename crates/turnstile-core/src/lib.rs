//! Webrify Turnstile core.
//!
//! Platform-agnostic verification logic shared between the WASM frontend and
//! the native Axum backend. Every module here is pure (no I/O), so it builds
//! identically for `wasm32-unknown-unknown` and native targets and is unit
//! testable without any mocking.
//!
//! Modules:
//! - [`rng`]: OS-backed cryptographic randomness (challenges, salts, ids).
//! - [`pow`]: Hashcash SHA-256 proof-of-work solver and verifier.
//! - [`protocol`]: wire types for the challenge/verify flow.
//! - [`fingerprint`]: 128-bit browser-fingerprint hashing (signals → stable id).
//! - [`risk`]: composite risk scoring (signals → score + allow/escalate/deny).

#![forbid(unsafe_code)]

pub mod fingerprint;
pub mod pow;
pub mod protocol;
pub mod risk;
pub mod rng;
