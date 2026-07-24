# Webrify Turnstile — Improvement Roadmap

## Implemented

| # | Improvement | Version | Category |
|---|-------------|---------|----------|
| P1 | `solve_time_ms` — client-side solve timing sent to server | v0.3.0 | Anti-abuse |
| P2 | Rate-limiter eviction — prevent unbounded HashMap growth | v0.3.0 | Stability |
| P3 | Separate rate limits — `/challenge` 3 req/s, `/verify` 10 req/s | v0.3.0 | Anti-abuse |
| P4 | Redis-backed distributed rate limiter (`INCR` + `EXPIRE`) | v0.3.4 | Scalability |
| P5 | Auto-tuning difficulty — median solve time feedback loop | v0.3.2 | UX + Anti-abuse |
| P6 | JWT `nbf` (not-before) + `protocol_version` field | v0.3.1 | Protocol |
| P7 | Multi-worker PoW — `Promise.any` over divided nonce space | v0.3.4 | Performance |
| P8 | PoW progress callback — time-based heartbeat every 200ms | v0.3.2 | UX |
| P9 | Structured Prometheus metrics — risk decisions, solve-time histogram, replay counter | v0.3.3 | Observability |
| P10 | Remove `idempotency_key` dead code | v0.3.1 | Cleanup |

## Pending — Tactical

| # | Improvement | Impact | Effort |
|---|-------------|--------|--------|
| P11 | Server-side solve-time validation — store `issued_at` in Redis | Medium | 1h |

### P11 — Server-side solve-time validation

**Current state**: Challenges are stateless (HMAC-signed, never stored in Redis at mint time). `solve_time_ms` already comes from the client (P1), but a determined attacker can lie about it.

**Change**: Store `webrify:issued:{challenge_hex}` with the issuance timestamp when minting a challenge. At verify time, compute `now - issued_at`. If `<100ms` for difficulty 14, escalate risk. Works even without trusting the client's `solve_time_ms`.

**Impact**: The client P1 can be bypassed by an attacker who sends a fake slow `solve_time_ms`. This closes that gap by having the server measure wall-clock independently.

**Architecture concern**: Adds a Redis write on every `/challenge` call (currently stateless). Needs graceful degradation if Redis is down (challenge still valid via HMAC, just no timing check).

## Pending — Strategic

### P12 — Memory-hard PoW (Argon2id)

**What**: Replace or supplement SHA-256 Hashcash with Argon2id.

**Why**: SHA-256 is purely compute-bound. GPU/ASIC achieves 1000-10000x speedup over browser WASM. Argon2 forces memory consumption (16-64MB) that saturates GPU memory bandwidth, making GPU *slower* than CPU for this workload.

**Comparison**:

| | SHA-256 Hashcash | Argon2id (64MB) |
|---|---|---|
| Bottleneck | Compute (FLOPS) | Memory bandwidth |
| Browser CPU | ~1s @ diff 14 | ~300ms |
| GPU (RTX 4090) | <1ms | ~2s (slower than CPU) |
| ASIC | <1µs | Not cost-effective (needs RAM per unit) |
| Parallelization | Near-linear | Sub-linear, bandwidth-saturated |

**Changes needed**:
- **Core**: New `pow.rs` functions — `verify_argon2id(seed, nonce, params)` with memory/iterations/parallelism params
- **WASM**: Compile `argon2` crate for `wasm32-unknown-unknown`, expose `solve_argon2id()` binding
- **Protocol**: `Challenge.algorithm` accepts `"Argon2id"`, new `params` field (`{ m: 65536, t: 3, p: 1 }`)
- **Widget**: Runtime capability detection — request Argon2 first, fall back to SHA-256
- **Server**: Dual-algorithm verification, configurable algorithm preference

**Costs**: +50KB WASM binary, 16-64MB per-tab memory during solve, mobile devices may prefer lower memory settings.

**Recommended parameters**: `m=16384` (16MB) for baseline, `m=65536` (64MB) for high-security deployments. `t=3` iterations, `p=1` thread (browser sandbox constraint).

### P13 — Verifiable Delay Function (VDF)

**What**: Replace Hashcash with a mathematical function that *cannot be parallelized* — must take exactly T sequential steps regardless of hardware.

**Why**: The gold standard for CAPTCHA PoW. Neither GPU nor ASIC nor distributed computing provides any advantage — everyone must wait the same wall-clock time. This is fundamentally stronger than memory-hard PoW because it doesn't depend on hardware constraints at all.

**How it works**: Repeated squaring in an RSA group — `x^(2^T) mod N`. Computing this requires T sequential squarings (no shortcut). Verifying takes O(log T) time using a Wesolowski proof.

**Why strategic**: Requires new cryptographic primitives (RSA group operations, Wesolowski proofs), large-integer WASM support, and a complete PoW model rethink. This is an academic-grade improvement, not a weekend task.

**Changes needed**:
- **Core**: VDF evaluation + Wesolowski proof generation/verification
- **WASM**: BigInt operations in WASM (via `num-bigint` or JS BigInt FFI)
- **Protocol**: New `algorithm: "VDF-Wesolowski"`, parameters `{ T: 100000, N_bits: 2048 }`
- **Server**: Proof verification in O(log T) not O(T)

### P14 — Ed25519 JWT (asymmetric signing)

**What**: Replace HS256 (symmetric HMAC-SHA256) with Ed25519 (asymmetric EdDSA).

**Why**: Currently the same key both signs and verifies tokens. For multi-tenant or third-party integration, verifiers need a public key without holding the signing secret. Ed25519 enables this.

**Why strategic**: HS256 is fine for single-tenant self-hosted deployment. This is an architecture upgrade for future use cases, not a security fix for the current model.

**Changes needed**:
- **jwt.rs**: Use `jsonwebtoken` with `Algorithm::EdDSA`, `EncodingKey::from_ed_der`, `DecodingKey::from_ed_der`
- **config.rs**: Add `jwt_public_key: Option<String>` — if set, Ed25519 mode; otherwise, HS256 compatibility
- **Key generation**: Document `openssl genpkey -algorithm ed25519` for keypair generation
- **Backward compatibility**: Existing HS256 tokens remain valid; new tokens use Ed25519 when configured

**Effort**: ~2-3 hours, the simplest of the strategic items.

## Recommended Priority

1. **P14** — Ed25519 JWT (simplest, enables multi-tenancy)
2. **P11** — Server-side solve-time validation (closes client-timing bypass gap)
3. **P12** — Argon2 memory-hard PoW (largest anti-abuse impact)
4. **P13** — VDF (academic-grade, long-term)
