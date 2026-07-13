# Webrify Turnstile — Operations & Integration Guide

Self-hosted, single-tenant human-verification (a Cloudflare-Turnstile-style
PoW + fingerprint + behavior + risk system). This document covers deployment,
configuration, embedding the widget on your pages, key management, and the
audit/security posture.

## 1. Architecture at a glance

- **`turnstile-core`** (Rust, compiles to both `wasm32` and native): the PoW
  engine, fingerprint hashing, behavior scoring, risk model, protocol types.
- **`turnstile-wasm`**: thin `wasm-bindgen` bindings loaded by the browser.
- **`turnstile-server`** (Axum binary `webrify`): mints & verifies challenges,
  issues JWTs, serves the embedded widget, exposes `/metrics`. The only external
  stateful dependency is **Redis** (challenge anti-replay + escalation counters).
- **`turnstile-widget`** (React + Vite): the embeddable UI, built into the
  server binary via `rust-embed` — no separate frontend deploy.

Single self-contained binary + Redis. That's the whole deployment.

## 2. Build & run

### Prerequisites
- Rust (stable; `rust-toolchain.toml` pins it + the `wasm32-unknown-unknown` target).
- `wasm-pack` (builds the WASM from `turnstile-core`).
- Node + npm (builds the widget; only at build time, not at run).
- Redis (the only runtime external dependency).

### Build (ordered pipeline)
```
just build-all        # wasm-pack -> vite (widget) -> cargo build (server, re-embeds widget)
```
Or step-by-step:
```
just build-wasm       # crates/turnstile-wasm -> pkg/
just build-widget     # copies pkg/ in, then vite build -> packages/turnstile-widget/dist/
cargo build -p turnstile-server   # rust-embed bakes dist/ into the binary
```
> `crates/turnstile-server/build.rs` watches `dist/` and emits
> `rerun-if-changed`, so a widget rebuild is picked up automatically on the
> next `cargo build` — no manual `cargo clean` needed.

### Run
```
redis-server &                              # or: brew services start redis
WEBRIFY_HMAC_KEY=...  WEBRIFY_JWT_KEY=...  \
WEBRIFY_ALLOWED_ORIGINS=https://yourapp.com \
./webrify
# -> listening on 127.0.0.1:3000 (configurable)
```

## 3. Configuration

Loaded once at startup (Parse-Don't-Validate). Sources, later-wins:
optional TOML file, then environment variables (prefix `WEBRIFY_`).

| Field                 | Env var                       | Default                       | Notes                                                                                  |
| --------------------- | ----------------------------- | ----------------------------- | -------------------------------------------------------------------------------------- |
| `bind_addr`           | `WEBRIFY_BIND_ADDR`           | `127.0.0.1:3000`              | Socket to bind.                                                                        |
| `redis_url`           | `WEBRIFY_REDIS_URL`           | `redis://127.0.0.1:6379/0`    | Anti-replay + escalation store.                                                        |
| `hmac_key`            | `WEBRIFY_HMAC_KEY`            | **(required)**                | Secret for HMAC-signing challenges.                                                    |
| `jwt_key`             | `WEBRIFY_JWT_KEY`             | **(required)**                | Secret for HS256-signing verification JWTs.                                            |
| `allowed_origins`     | `WEBRIFY_ALLOWED_ORIGINS`     | **(required)**                | Comma-separated in env; TOML array also accepted. CORS + challenge binding.            |
| `difficulty`          | `WEBRIFY_DIFFICULTY`          | `14`                          | Base PoW difficulty (leading-zero bits). ~14 ≈ 1s desktop. Capped at 24.               |
| `maxnumber`           | `WEBRIFY_MAXNUMBER`           | `100_000`                     | PoW nonce search-space cap (client-side bound).                                        |
| `challenge_ttl_secs`  | `WEBRIFY_CHALLENGE_TTL_SECS`  | `300`                         | Challenge lifetime.                                                                    |
| `jwt_ttl_secs`        | `WEBRIFY_JWT_TTL_SECS`        | `900`                         | Issued JWT lifetime.                                                                   |
| `allow_js_disabled`   | `WEBRIFY_ALLOW_JS_DISABLED`   | `false`                       | Fail-closed by default; `true` enables a no-PoW high-risk path (not in MVP flow).       |

### TOML form (`webrify.toml`)
```toml
bind_addr = "0.0.0.0:3000"
redis_url = "redis://redis:6379/0"
hmac_key = "..."      # or set via env only
jwt_key = "..."
difficulty = 12
allowed_origins = ["https://app.example.com", "https://www.example.com"]
```
Pass the path with a small code change (`Config::load(Some(path))`); today main
loads env-only — point it at a file if you prefer TOML.

> **Managing `allowed_origins`**: there's no first-class CLI yet (tracked);
> edit the TOML / restart. A `webrify sitekey` subcommand is the planned
> enhancement.

## 4. Embedding the widget

The widget is served from the binary at `/widget/turnstile.js`. On a protected
page, add a container and mount it:

```html
<!-- anywhere in your page -->
<div id="webrify-ts"></div>

<!-- load the widget (same binary, different origin, or a CDN you mirror) -->
<script type="module">
  import { mount } from "https://webrify.yourhost/widget/turnstile.js";

  mount(document.getElementById("webrify-ts"), {
    endpoint: "https://webrify.yourhost",   // empty string = same origin
    onVerify: (token) => {
      // send `token` to YOUR backend; your backend trusts it (HS256-signed,
      // bound to the origin, short-lived) or re-validates via a future
      // /siteverify endpoint.
      fetch("/your-login", { method: "POST", body: JSON.stringify({ token }) });
    },
    onError: (msg) => console.warn("turnstile:", msg),
    // disableFingerprint: true,   // GDPR no-fingerprint / PoW-only fallback
  });
</script>
```

The widget:
1. `POST {endpoint}/challenge` (browser sends the page's `Origin` header —
   must be in `allowed_origins`).
2. Computes a fingerprint (Canvas/WebGL/Audio/navigator) and a behavior score
   (mouse/keystroke/click CV analysis) **in a Web Worker via WASM**, then solves
   the PoW off the main thread.
3. `POST {endpoint}/verify` with the nonce + fingerprint + behavior score →
   receives a short-lived JWT.
4. Calls `onVerify(token)`.

### React usage
If your host is React, import the component directly:
```tsx
import { TurnstileWidget } from "webrify-turnstile"; // if published as a pkg
<TurnstileWidget endpoint="https://webrify.yourhost"
                 onVerify={t => setToken(t)} />
```

### Content-Security-Policy for host pages
Because the widget loads a script + spawns a Worker + instantiates WASM, a strict
host CSP must allow:
```
script-src https://webrify.yourhost;
worker-src https://webrify.yourhost;     // the PoW worker chunk
connect-src https://webrify.yourhost;    // /challenge + /verify fetches
wasm-unsafe-eval;                         // WASM instantiation (PoW solver)
```

## 5. Key management & rotation

- **`hmac_key`** signs challenges; **`jwt_key`** signs verification JWTs
  (HS256). Both are symmetric secrets kept server-side only.
- **Rotate** by changing the env/TOML value and restarting. Outstanding
  challenges signed with the old HMAC are rejected post-restart (operators
  should rotate in a maintenance window or briefly tolerate in-flight
  challenges).
- A `kid` (key id) in the JWT header is **not yet** set; adding one lets you
  support overlapping old/new keys during rotation without invalidating
  in-flight tokens. Tracked as a Phase-4 hardening item.
- Never commit secrets. Load from env / a secrets manager; the binary refuses to
  start if `hmac_key` / `jwt_key` / `allowed_origins` are empty.

## 6. Audit & data posture

- **Ephemeral by design.** No durable per-attempt records: challenges,
  spent-nonce markers, and escalation counters all live in Redis with TTLs and
  are never persisted to disk by Webrify.
- **Fingerprint minimization (GDPR).** Raw environment signals (Canvas/WebGL/
  Audio/fonts) are hashed **in the browser**; only the 128-bit hash leaves the
  client, and even that can be disabled (`disableFingerprint`) for a PoW-only
  path. No cookies are set.
- **Forensics.** Structured JSON logs (`RUST_LOG=info`) + `/metrics` counters
  give aggregate visibility (success/block rate). For per-attempt queryable
  history, ship the JSON log stream to an aggregator (Loki/ELK) with retention.
  If a single tenant later needs queryable per-attempt storage, add Postgres
  (the system currently has no DB).

## 7. Security posture

- **Fail-closed**: Redis-down returns `503` and refuses verification rather than
  silently bypassing anti-replay. `/ready` reflects Redis health so a load
  balancer pulls an unhealthy instance.
- **Anti-replay**: each challenge is single-use via an atomic Redis
  `SET … NX EX` (TOCTOU-safe).
- **Tamper-proof challenges**: the server HMAC-signs every binding parameter
  (`algorithm|salt|challenge|difficulty|maxnumber|expires_at|origin`); the client
  can't relax difficulty or swap origins. Verified constant-time.
- **PoW bound to fingerprint**: a solution can't be shared across clients — each
  environment must do its own work.
- **Rate limiting**: per-IP fixed window (10 req/s/IP default), applied in
  production only.
- **Adaptive difficulty**: peers flagged by the risk model get progressively
  harder challenges (each bit ≈ 2× cost), capped so legitimate users aren't
  locked out.
- `#![forbid(unsafe_code)]` across the security-critical Rust crates.

## 8. HTTP endpoints

| Method | Path          | Purpose                                                          |
| ------ | ------------- | ---------------------------------------------------------------- |
| POST   | `/challenge`  | Mint an HMAC-signed challenge (requires `Origin` ∈ allowlist).   |
| POST   | `/verify`     | Submit nonce + fingerprint + behavior → JWT (or an error).       |
| GET    | `/health`     | Liveness (process up).                                           |
| GET    | `/ready`      | Readiness (Redis reachable); 503 if down (fail-closed).          |
| GET    | `/metrics`    | Prometheus text exposition (counters).                           |
| GET    | `/widget/*`   | Embedded widget assets (JS/worker/wasm).                         |
| GET    | `/demo`       | Same-origin demo page exercising the widget.                     |
