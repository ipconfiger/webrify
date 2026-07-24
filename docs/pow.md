# Proof-of-Work Mechanism Analysis

## Current Architecture

**Algorithm**: Hashcash-style SHA-256 PoW, computed in browser via WASM Web Worker, verified server-side by the turnstile server.

### End-to-End Flow

```
Browser (Widget)                          Webrify Server                    Redis
     │                                        │                               │
     │  POST /challenge (Origin header)       │                               │
     │ ──────────────────────────────────────>│                               │
     │                                        │ escalation_count(ip) ───────>│
     │                                        │ <── raw count (capped at 6   │
     │                                        │      in adjust_difficulty) ── │
     │                                        │                               │
     │                                        │ adjust_difficulty(base=14, es  │
     │                                        │   c.min(MAX_ESCALATION_BITS)) │
     │                                        │ salt (128-bit CSPRNG)
     │                                        │ HMAC(all fields)              │
     │ <── Challenge {seed, diff, maxnumber,  │                               │
     │       salt, origin, expires, sig} ─────│                               │
     │                                        │                               │
     │ fingerprint_hash(signals) → 128-bit    │                               │
     │ behavior recorder snapshot             │                               │
     │ Web Worker: solve_bounded(seed||fp,    │                               │
     │   difficulty, maxnumber) → nonce       │                               │
     │ behavior_score(telemetry) → 0-100      │                               │
     │                                        │                               │
     │  POST /verify {all fields echoed +     │                               │
     │    nonce, fingerprint, behavior_score}  │                               │
     │ ──────────────────────────────────────>│                               │
     │                                        │ 1. HMAC re-verify (constant-time)
     │                                        │ 2. Origin allowlist check     │
     │                                        │ 3. Expiry check               │
     │                                        │ 4. claim_spent(): SET NX EX ──>│
     │                                        │    <── OK/AlreadyUsed ──────── │
     │                                        │ 5. pow::verify(seed, nonce, diff)
     │                                        │ 6. nonce ≤ maxnumber           │
     │                                        │ 7. risk::evaluate()→Allow/Escalate/Deny
     │                                        │ 8. Escalate/Deny → record_escalation(ip)─>│
     │                                        │ 9. Deny → reject              │
     │                                        │ 10. Issue HS256 JWT           │
     │ <── {token, expires_at} ───────────────│                               │
```

### Key Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| Base difficulty | 14 | ~2^14 ≈ 16K expected hashes |
| Escalation bits | +1 per escalation, max +6 | Doubles work per bit (max 2^20 = ~1M hashes) |
| Difficulty cap | 24 | Absolute upper bound |
| maxnumber | 100,000 | Search space cap (prevents infinite loops) |
| Challenge TTL | 300s (configurable) | How long a challenge is valid |
| Challenge seed | 128-bit CSPRNG | Per-challenge random seed |
| Salt | 128-bit CSPRNG | Per-challenge random salt |
| JWT algorithm | HS256 | Symmetric HMAC-SHA256 |

### Seed Construction

- **With fingerprint**: `seed = hex_decode(challenge_hex) || hex_decode(fingerprint_hex)`
- **GDPR mode** (disableFingerprint): `seed = hex_decode(challenge_hex)` only

### Difficulty Formula

```
effective_difficulty = min(24, base_difficulty + min(6, escalation_count))
```

Each escalation adds +1 bit (double the work), capped at +6 bits and overall difficulty 24.

### Risk Scoring (server-side)

`RiskInput` has 4 fields: `challenge_passed`, `fingerprint_blacklisted`, `solve_time_ms`, `behavior_score`. Decision thresholds: `<30` = Allow, `30–69` = Escalate, `≥70` = Deny.

| Signal | Threshold | Risk Points | Decision (isolated) |
|--------|-----------|-------------|---------------------|
| challenge_passed | false | — | Deny (early return) |
| fingerprint_blacklisted | true | +80 | Deny |
| solve_time_ms | < 20ms | +75 | Deny |
| solve_time_ms | < 50ms | +30 | Escalate |
| behavior_score | < 30 | +50 | Escalate |
| behavior_score | < 60 | +20 | Allow (if alone) |

Note: `challenge_passed=false` short-circuits: verification is rejected before risk scoring (`verify.rs:86-88`). The remaining signals are additive — multiple warnings can push a single Escalate-range signal into Deny territory.

## Strengths

| Category | Detail |
|----------|--------|
| **Defense-in-depth** | HMAC covers ALL binding fields independently of Redis replay check — tampered fields rejected before storage access |
| **Fingerprint binding** | PoW seed = `challenge || fingerprint` — solution is non-transferable across devices (except GDPR mode) |
| **Constant-time HMAC** | `Mac::verify_slice` prevents timing oracle on signature check |
| **TOCTOU-safe replay** | Atomic `SET NX EX` — no window between check-and-mark |
| **Fail-closed** | Redis errors → 503 (never silently bypass anti-replay), escalation tracking is the only fail-open path |
| **WASM + Web Worker** | PoW loop never freezes the UI; fingerprint/behavior also in WASM |
| **GDPR-aware** | Raw fingerprint signals never leave browser — only 128-bit hash; `disableFingerprint` mode skips collection entirely |
| **Platform-agnostic core** | Same `turnstile_core` code runs native and wasm32 — no conditional compilation |
| **Proper entropy** | 128-bit CSPRNG seed per challenge |
| **Behavior analysis** | Four composite signals: timing CV, angular jitter, straightness, click coherence |
| **Well-tested** | `solve`/`verify` are pure functions with property-based tests |

## Weaknesses & Gaps

### Security

#### G1. GPU/ASIC farming viable — no memory hardness (HIGH)

Hashcash over SHA-256 is purely compute-bound. A GPU (or ASIC) can achieve orders-of-magnitude more hashes/second than browser WASM. There is no memory-hard component (scrypt, Argon2, Equihash, RandomX) to level the playing field between CPU and GPU/ASIC.

**Impact**: A single GPU can solve difficulty-14 challenges in microseconds vs. ~1 second on a browser CPU.

#### G2. `solve_time_ms` is designed but never populated (HIGH)

`RiskInput.solve_time_ms` has well-calibrated thresholds (`<20ms → +75 risk`, `<50ms → +30 risk`), but the client **never sends it** and the server **never measures server-side latency**. This is the strongest anti-GPU signal and it's completely dead.

**Impact**: The most direct detection of GPU-accelerated solvers is unused.

#### G3. GDPR mode makes solutions transferable (HIGH)

When `disableFingerprint` is true, the PoW seed is just `hex_decode(challenge_hex)` — identical for all clients. A bot operator can solve once, distribute the nonce to thousands of headless browsers, and all pass verification.

**Impact**: Core anti-sharing defense is absent in GDPR mode.

#### G4. Behavior telemetry is client-side only — spoofable (MEDIUM)

All four behavior signals are generated by JS event listeners. A bot can trivially synthesize realistic traces — there's zero server-side validation that events came from real user input.

**Impact**: Raises the bar slightly (synthesis requires effort) but provides no cryptographic assurance.

#### G5. Idempotency key is dead code (LOW)

`VerifyRequest.idempotency_key` is sent by the client but the server never reads or validates it. The actual replay protection is via `SET NX EX` on the challenge field alone.

**Impact**: Low (real replay protection works). But misleading — should be removed or implemented.

#### G6. JWT uses symmetric HS256 (LOW)

For self-hosted single-tenant this is fine. Any future multi-tenant or public-verifier use case requires asymmetric keys (Ed25519/ES256).

### Rate Limiting & Anti-Abuse

#### G7. Rate limiter is in-process only — doesn't scale horizontally (MEDIUM)

`rate_limit.rs` stores per-IP windows in a `Mutex<HashMap<IpAddr, Window>>`. Behind a load balancer with N replicas, an attacker gets N× the rate limit.

**Impact**: Medium for single-instance, high for scaled deployments.

#### G8. Rate limiter map has unbounded growth — memory leak (MEDIUM)

Stale entries are evicted only when they're checked again. An IP that hits once and never returns occupies memory forever. Under sustained attack from rotating IPs, the map grows without bound.

**Impact**: DoS vector against the server process under sustained attack.

#### G9. Fixed-window rate limiter allows boundary bursts (LOW-MEDIUM)

A fixed 1-second window means a burst of 10 at t=0.99s followed by 10 at t=1.01s yields 20 requests in ~20ms. A sliding window or token bucket would be stricter.

#### G10. No separate rate limits for `/challenge` vs `/verify` (MEDIUM)

`/challenge` is cheap (no Redis write, just HMAC + CSPRNG). `/verify` is expensive (Redis `SET NX EX` + PoW verify). Both share the same 10 req/s/IP limit. An attacker can mint unlimited challenges offline, solve them with GPU, and burst `/verify`.

#### G11. No progressive delay on failed verifications (LOW-MEDIUM)

If a client submits wrong nonces, there's no escalating backoff. Many CAPTCHAs apply exponential backoff on consecutive failures.

### Performance

#### G12. WASM PoW is single-threaded (MEDIUM)

`solve_bounded` is a simple linear scan in a single Web Worker. Modern browsers support `SharedArrayBuffer` + multiple workers.

**Impact**: Hurts legitimate users (locked to one core) more than attackers (who can run many tabs/processes).

#### G13. `maxnumber` is large and static (LOW)

Default `maxnumber`: 100,000. For difficulty 14 (expected ~16K iterations), a 100K cap is generous. But the value doesn't adapt to solve-time observations.

#### G14. No auto-tuning of difficulty from observed solve times (MEDIUM)

The difficulty is static (plus escalation offset). There's no feedback loop: if legitimate clients average 1.2s to solve, the system won't reduce; if solve times drop to 50ms, it won't increase.

### Observability

#### G15. Metrics are too coarse (MEDIUM)

Only 3 counters: `challenges_issued`, `verifies_success`, `verifies_failed`. Missing: solve-time histograms, risk score distributions, escalation counts, replay-attempt counters, Redis error rate, rate-limit hits.

#### G16. No structured log correlation ID (LOW)

No request-scoped correlation ID that flows through all server-side steps — hard to debug individual verification failures end-to-end.

### Protocol

#### G17. No algorithm versioning (LOW)

The `Challenge.algorithm` field is always `"SHA-256"` — but there's no protocol version negotiation. If a future version introduces memory-hard PoW, clients must know before requesting challenges.

#### G18. `maxnumber` precision boundary with JSON (LOW)

`maxnumber` is a `u64` sent as `number` over JSON. JS `number` loses precision above 2^53. For realistic values this is fine, but the cast should document the constraint.

### Additional Weaknesses (from code audit)

#### G19. No timeout on PoW solve — browser can hang indefinitely (MEDIUM)

`pow-worker.ts:35` calls `solve_challenge()` with no `setTimeout` guard. `useTurnstile.ts:85-86` creates the worker promise with no `Promise.race(timeout)`. A misconfigured server or attacker controlling `maxnumber` can freeze the user's browser tab.

#### G20. No retry logic for transient failures (LOW-MEDIUM)

`useTurnstile.ts:145-195`: The `verify()` flow fetches challenge, solves, and verifies in a single linear flow. A 503 from Redis unavailability fails the entire flow with no automatic retry. The user must manually re-trigger.

#### G21. `fingerprint_hash` truncates to 128 bits for binding (LOW)

`fingerprint.rs:30-37`: The fingerprint hash truncates SHA-256 to 128 bits for PoW seed extension. Two different browsers with signals hashing to the same 128-bit prefix (birthday bound ~2^64) would have transferable solutions. 256-bit truncation costs nothing and avoids this entirely.

#### G22. No circuit breaker for Redis failures (MEDIUM)

`store.rs:51-60`: Every `/verify` call attempts Redis. If Redis is down, every call fails with 503. There's no circuit breaker to short-circuit after N consecutive failures.

#### G23. Double-copy in WASM data path (LOW)

`wasm/lib.rs:79-80`: Behavior data buffers are transferred from main thread to worker (zero-copy via `ArrayBuffer`), but the WASM binding calls `.to_vec()` which copies from JS typed array memory into WASM linear memory — a redundant second copy.

#### G24. Server-side verify comment is outdated

`verify.rs:3-12`: The header comment lists 7 steps but omits risk evaluation, escalation recording, and deny rejection (steps 7-9 in the actual code).

## Prioritized Improvements

### Priority 1: Critical (High Impact, Low Effort)

#### P1. Populate `solve_time_ms` — 30 min

Track wall-clock time from challenge fetch to solve completion on the client side and send it.

```typescript
// In useTurnstile.ts
const t0 = performance.now();
const chalRes = await fetch(`${endpoint}/challenge`, { method: "POST" });
const challenge = (await chalRes.json()) as Challenge;
// ... solve ...
const solveTimeMs = Math.round(performance.now() - t0);
// Include solve_time_ms in the /verify body
```

In `VerifyRequest` (`protocol.rs`): add `solve_time_ms: Option<u64>` field.
In `verify.rs`: pass `solve_time_ms` to `RiskInput`.

The risk thresholds are already calibrated:
- `<20ms` → +75 risk → Deny (only GPU achievable)
- `<50ms` → +30 risk → Escalate

#### P2. Fix rate-limiter unbounded growth — 15 min

Add periodic cleanup to `rate_limit.rs`:

```rust
const MAX_ENTRIES: usize = 100_000;
if windows.len() > MAX_ENTRIES {
    windows.retain(|_, w| now.duration_since(w.start) < self.window);
    if windows.len() > MAX_ENTRIES {
        windows.clear();
    }
}
```

#### P3. Separate rate limits for /challenge vs /verify — 20 min

Challenge minting is cheap but enables offline GPU farming. Apply stricter limit to `/challenge`:

```rust
let challenge_limiter = Arc::new(RateLimiter::new(Duration::from_secs(1), 3));
let verify_limiter = Arc::new(RateLimiter::new(Duration::from_secs(1), 10));
```

### Priority 2: High Impact

#### P4. Redis-backed distributed rate limiter — 1-2h

Replace in-process limiter with Redis-based sliding window:

```rust
pub async fn check_rate_limit(&self, ip: IpAddr, max: u32, window_secs: u64) -> Result<bool, RedisError> {
    let key = format!("webrify:rate:{ip}");
    let count: u32 = redis::cmd("INCR").arg(&key).query_async(conn).await?;
    if count == 1 {
        let _: () = redis::cmd("EXPIRE").arg(&key).arg(window_secs).query_async(conn).await?;
    }
    Ok(count <= max)
}
```

Fail-open on Redis error (rate limiting is enhancement, not security guarantee).

#### P5. Auto-tuning difficulty from observed solve times — 2-3h

Rolling window of recent legitimate solve times in Redis. When minting a challenge:

```rust
let median_solve_ms = state.store.recent_solve_median().await.unwrap_or(1000);
let tuned_base = if median_solve_ms < 100 { cfg.difficulty + 1 }
                 else if median_solve_ms > 3000 { cfg.difficulty.saturating_sub(1) }
                 else { cfg.difficulty };
let difficulty = pow::adjust_difficulty(tuned_base, escalations);
```

Target: ~1 second solve time for legitimate users.

#### P6. Add `nbf` to JWT + protocol version field — 30 min

- Add `nbf: now` to `JwtClaims`
- Add `validation.set_required_spec_claims(&["exp", "nbf"])`
- Add `protocol_version: u32` field to `Challenge` and `VerifyRequest`

### Priority 3: Medium Impact

#### P7. Multi-threaded WASM PoW — 4-6h

Split nonce search across multiple Web Workers using `SharedArrayBuffer` + `Atomics`. For difficulty 14 with 4 workers, each searches 0-25K, 25K-50K, etc. First valid nonce wins.

#### P8. Progress callback from WASM to JS — 1-2h

Fire callback every N iterations (e.g., every 1000) so the widget can render a progress bar. Also helps detect "stuck" solves.

#### P9. Structured risk metrics — 1h

Extend `metrics.rs` with:
- Solve time histogram (buckets: <10ms, <50ms, <200ms, <1s, <5s, >5s)
- Counter per `Decision` variant (Allow, Escalate, Deny)
- Counter for replay attempts
- Gauge for escalation count distribution

#### P10. Implement or remove idempotency key — 30 min

Either remove `idempotency_key` (dead code), or implement deduplication with `SET NX EX` on `webrify:idem:{key}`.

#### P11. Server-side solve-time validation — 1h

Store `issued_at` with the challenge key in Redis. At verify time, check `now - issued_at`. If <100ms for difficulty 14, escalate risk. This works even without client reporting `solve_time_ms`.

**Note**: Challenges are currently stateless (HMAC-signed, never stored in Redis at mint time). This improvement requires writing challenge metadata at mint time — a non-trivial architecture change.

### Priority 4: Strategic (Longer-term)

#### P12. Memory-hard PoW (Argon2 or Equihash)

Replace or supplement SHA-256 Hashcash with a memory-hard function. Argon2id with modest memory (16-64MB) makes GPU farming significantly harder while remaining feasible in browser WASM. New `algorithm` field value for protocol negotiation.

#### P13. Verifiable Delay Function (VDF)

A VDF (e.g., repeated squaring in a group of unknown order) provides inherent time delay. Unlike Hashcash, a VDF cannot be parallelized — ASIC/GPU provides no advantage. The gold standard for CAPTCHA PoW.

#### P14. Ed25519 JWT for public-key verification

Replace HS256 with Ed25519 so relying applications can verify tokens with a public key. Enables multi-tenant deployments and third-party integration.

## Summary Priority Matrix

| # | Improvement | Impact | Effort | Category | Status |
|---|-------------|--------|--------|----------|--------|
| P1 | Populate `solve_time_ms` | Critical | 30m | Anti-abuse | ✅ v0.3.0 |
| P2 | Fix rate-limiter unbounded growth | Critical | 15m | Stability | ✅ v0.3.0 |
| P3 | Separate rate limits per route | Critical | 20m | Anti-abuse | ✅ v0.3.0 |
| P4 | Redis-backed distributed rate limiter | High | 1-2h | Scalability | |
| P5 | Auto-tuning difficulty | High | 2-3h | UX + Anti-abuse | ✅ done |
| P6 | Add `nbf` to JWT + protocol version | High | 30m | Protocol hardening | ✅ v0.3.1 |
| P7 | Multi-threaded WASM PoW | Medium | 4-6h | Performance | |
| P8 | Progress callback for PoW solving | Medium | 1-2h | UX | |
| P9 | Structured risk metrics | Medium | 1h | Observability | |
| P10 | Implement-or-remove idempotency key | Medium | 30m | Protocol cleanup | ✅ v0.3.1 |
| P11 | Server-side solve-time validation | Medium | 1h | Anti-abuse | |
| P12 | Memory-hard PoW (Argon2) | Strategic | Days | Anti-abuse | |
| P13 | VDF for inherent time delay | Strategic | Weeks | Anti-abuse | |
| P14 | Ed25519 JWT | Strategic | 2-3h | Multi-tenancy |

**v0.3.0 shipped**: P1, P2, P3.
**v0.3.1 shipped**: P6, P10.
