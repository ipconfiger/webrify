# PoW CAPTCHA / Human-Verification Systems — Research Report (July 2026)

**Objective**: Survey open-source PoW-based human-verification systems to inform building a Cloudflare-Turnstile alternative using Rust + WASM + TypeScript.

---

## Table of Contents

1. [Friendly Captcha](#1-friendly-captcha)
2. [mCaptcha](#2-mcaptcha)
3. [Anubis](#3-anubis)
4. [ALTCHA](#4-altcha)
5. [Cloudflare Turnstile (public architecture)](#5-cloudflare-turnstile)
6. [Synthesis & Best Practices](#6-synthesis--best-practices)
7. [Recommended Protocol Schema](#7-recommended-protocol-schema)

---

## 1. Friendly Captcha

**URLs**: [friendlycaptcha.com](https://friendlycaptcha.com) · [GitHub: friendly-pow](https://github.com/friendlycaptcha/friendly-pow) · [GitHub: friendly-challenge (widget)](https://github.com/FriendlyCaptcha/friendly-challenge) · [GitHub: friendly-docs](https://github.com/FriendlyCaptcha/friendly-docs) · [GitHub: friendly-lite-server](https://github.com/FriendlyCaptcha/friendly-lite-server) · [Developer Hub](https://developer.friendlycaptcha.com)

### 1.1 PoW Algorithm

- **Hash function**: BLAKE2b-256 (from `friendly-pow` repo README)
- **What is hashed**: A 128-byte buffer composed of the 32–64 byte puzzle buffer (containing timestamp, account ID, app ID, version, expiry, solution count, difficulty, reserved bytes, nonce, optional data) **padded with zeroes to 128 bytes**. The last 8 bytes of this 128-byte buffer are the **solution** that the client varies.
- **Difficulty encoding**: A single byte `d` in the puzzle buffer. The **difficulty threshold** is computed as:  
  `T = floor(2^((255.999 - d) / 8))`  
  Interpreted as a little-endian unsigned 32-bit integer. The hash is valid if the first 4 bytes of the BLAKE2b-256 digest, read as LE u32, are `< T`.
- **Multiple solutions**: To reduce variance, the client finds `n` solutions (encoded as a byte in the buffer) at a lower difficulty threshold. Final solution = `n` × 8-byte values concatenated.
- **Solver**: WASM (AssemblyScript) with JS fallback; WASM is ~10× faster.

### 1.2 Challenge Issuance Endpoint

**v1 Endpoint**: `POST https://api.friendlycaptcha.com/api/v1/puzzle`?  
**v2 Endpoint**: `POST https://global.frcapi.com/api/v2/captcha/puzzle`

- **Request body**: Contains `sitekey` (the public site identifier).
- **Response**: A base64-encoded "puzzle" string in format:  
  `<signature>.<base64(puzzle_buffer)>`  
  where `puzzle_buffer` is a 32–64 byte binary encoding of: `[4B timestamp][4B account_id][4B app_id][1B version][1B expiry][1B num_solutions][1B difficulty][8B reserved][8B nonce][0-32B optional_data]`.
- **Signature**: An HMAC or asymmetric signature over the puzzle buffer. The base64 string contains the buffer, and the server-side signature is sent alongside.

### 1.3 Verification Endpoint

**v1**: `POST https://api.friendlycaptcha.com/api/v1/siteverify`  
**v2**: `POST https://global.frcapi.com/api/v2/captcha/siteverify`

**v1 Request** (JSON/form):
```json
{
  "solution": "<signature>.<puzzle_b64>.<solutions_b64>.<diagnostics_b64>",
  "secret": "<API_KEY>",
  "sitekey": "<optional_sitekey>"
}
```

**v2 Request** (JSON/form):
```json
{
  "response": "<solution_string>",
  "sitekey": "<optional>"
}
```
With header: `X-API-Key: <API_KEY>`

**v2 Response** (success):
```json
{
  "success": true,
  "data": {
    "event_id": "ev_...",
    "challenge": {
      "timestamp": "2026-02-05T13:01:25Z",
      "origin": "example.com"
    },
    "risk_intelligence": null
  }
}
```

**v2 Response** (failure):
```json
{
  "success": false,
  "error": {
    "error_code": "response_invalid",
    "detail": "..."
  }
}
```

Error codes: `response_invalid`, `response_timeout`, `response_duplicate`, `sitekey_invalid`, `auth_invalid`, `bad_request`.

### 1.4 Challenge Binding

- **Signing**: The puzzle buffer is signed with a server-side secret (HMAC or asymmetric key). The signature travels with the puzzle as plaintext (`<signature>.<puzzle_b64>`).
- **Server state**: Requires replay-tracking storage. The server must check whether a puzzle ID/nonce has been seen before.
- **TTL**: Encoded in the puzzle buffer as `expiry_byte × 300 seconds` (5-minute increments). Maximum ~21h. Widget auto-refreshes puzzles 30s before expiry.
- **Version byte**: Currently `1`.

### 1.5 Difficulty Adjustment

- **Adaptive**: In the cloud service, difficulty is based on risk assessment of the visitor (signals collected by the widget). The lite-server uses fixed difficulty.
- **Diagnostics**: Client sends 3 bytes of diagnostics (solve time, etc.) to help the server tune difficulty.

### 1.6 Anti-Replay

- The server tracks used puzzle nonces (replay check). Uses single-use tokens: once verified, the same puzzle cannot be verified again.
- Short TTL prevents long-window replay.

### 1.7 Signals Collected

- **PoW only (no fingerprinting)**: The widget does not collect fingerprints. The cloud service separately does risk assessment. Lite-server is pure PoW.
- The lite-server checks: signature, puzzle integrity, timestamps, replay, solution count, solution validity.

---

## 2. mCaptcha

**URLs**: [GitHub: mCaptcha](https://github.com/mCaptcha/mCaptcha) · [GitHub: pow_sha256 (Rust crate)](https://github.com/mCaptcha/pow_sha256) · [GitHub: pow_wasm](https://github.com/mCaptcha/pow_wasm) · [Docs](https://mcaptcha.github.io/mCaptcha/) · [ACM Paper](https://cacm.acm.org/research/mcaptcha-replacing-captchas-with-rate-limiters-to-improve-security-and-accessibility/)

### 2.1 PoW Algorithm

- **Hash function**: SHA-256
- **What is hashed**: `SHA256(SALT || serialized_input_T || nonce)` where:
  - `SALT` = a 32+ byte server-configured salt (prevents cross-system PoW reuse)
  - `serialized_input_T` = serde-serialized challenge string (one-time phrase)
  - `nonce` = incrementing counter (u64)
- **Difficulty encoding**: The first 16 bytes of the SHA-256 digest are interpreted as a **128-bit unsigned integer**. The proof is valid if `result >= difficulty` (or equivalently, the result must exceed a threshold). In practice the difficulty is encoded as u32.
- **Hashcash-style**: `nonce` starts at 0 and increments until the hash meets the difficulty requirement.

### 2.2 Challenge Issuance Endpoint

**`POST /api/v1/pow/config`**

Request body:
```json
{
  "key": "abcdef1234567890abcdef1234567890"
}
```

Response (200):
```json
{
  "string": "rAnDoMcHaLlEnGeStRiNg",
  "difficulty_factor": 50000,
  "salt": "saltvaluehere",
  "max_recorded_nonce": 312847
}
```

- `string` = the one-time challenge phrase (serialized input T)
- `difficulty_factor` = u32 difficulty (higher = more work)
- `salt` = the global salt for PoW computation
- `max_recorded_nonce` = statistical max nonce from prior solves
- Challenge lifetime: **30 seconds** default (configurable via `duration`)

### 2.3 Verification & Token Issuance

**`POST /api/v1/pow/siteverify`** (internal to mCaptcha server)

The client sends a `Work` payload:
```json
{
  "string": "rAnDoMcHaLlEnGeStRiNg",
  "result": "<u128_as_number>",
  "nonce": 123456,
  "key": "<sitekey>"
}
```

The mCaptcha server verifies:
1. Fetches the `DifficultyFactor` and `SALT` for the site
2. Checks the challenge string is still valid (not expired, not replayed)
3. Recomputes `SHA256(SALT || string || nonce)` and checks `result >= difficulty_factor`
4. If valid, issues a **time-constrained, single-use access token**

The website server then validates this access token via another endpoint (`/api/v1/verify`) before processing the user's request.

### 2.4 Challenge Binding

- **Salt-based**: A server-wide `SALT` is configured per mCaptcha instance (must be "long and random"). This prevents PoW reuse across different systems.
- **Challenge strings** are one-time and tracked via a HashCache (in-memory/bbolt) to prevent replay.
- **Server state required**: The `key` (sitekey) maps to stored difficulty levels and salt. Challenge strings are tracked for expiration and single-use.
- **No HMAC-based signing** of challenges (unlike ALTCHA or Friendly). Security relies on the non-predictability of the challenge string and the SALT.

### 2.5 Difficulty Adjustment

- **Adaptive / traffic-based**: mCaptcha uses a **leaky bucket** traffic model. Administrators configure multiple **levels** with visitor thresholds:
  ```json
  {
    "levels": [
      {"visitor_threshold": 100,    "difficulty_factor": 50000},
      {"visitor_threshold": 1000,   "difficulty_factor": 3000000},
      {"visitor_threshold": 10000,  "difficulty_factor": 5000000}
    ]
  }
  ```
- As the visitor count rises through thresholds, difficulty increases automatically (target solve time: ~0–2 seconds under attack).
- Strategies include: `avg_traffic`, `peak_sustainable_traffic`, `broke_my_site_traffic` — each with configurable difficulty and target time.

### 2.6 Anti-Replay

- Proof-of-work configurations have **short lifetimes** (30s default)
- **Single-use**: Each challenge string can be used only once; tracked in HashCache
- **Garbage collection**: Configurable GC period (default 30s) to clean expired entries
- Access tokens are also single-use and time-bound

### 2.7 Signals Collected

- **Pure PoW**: mCaptcha does **no fingerprinting**. It explicitly prides itself on being cookie-free and IP-address-independent.
- The sole signal is the client's ability to produce a valid SHA-256 PoW for the given challenge.

### 2.8 Rust Crates

- [`mcaptcha_pow_sha256`](https://docs.rs/mcaptcha_pow_sha256) — Core SHA-256 PoW library. Provides `Config` (salt), `prove_work()`, `is_valid_proof()`, `is_sufficient_difficulty()`.
- [`pow_wasm`](https://mcaptcha.github.io/pow_wasm) — WASM bindings for client-side PoW generation.

---

## 3. Anubis

**URLs**: [GitHub: TecharoHQ/anubis](https://github.com/TecharoHQ/anubis) · [Docs: how-anubis-works](https://github.com/TecharoHQ/anubis/blob/main/docs/docs/design/how-anubis-works.mdx) · [Docs: why-proof-of-work](https://github.com/TecharoHQ/anubis/blob/main/docs/docs/design/why-proof-of-work.mdx) · [API docs](https://techarohq-anubis.mintlify.app/api/anubis)

### 3.1 PoW Algorithm

- **Hash function**: SHA-256
- **What is hashed**: `SHA256(challenge_string || nonce)` where:
  - `challenge_string` = random hex data generated from request metadata (`Accept-Encoding`, `Accept-Language`, `X-Real-IP`, current time, and a 19-byte random key) — all concatenated and SHA-256-hashed to form the base challenge
  - `nonce` = integer counter starting at 0
- **Difficulty encoding**: The resulting hex digest must start with `difficulty` number of leading **zero hex nibbles** (Hashcash-style). Default difficulty: **4** (meaning the hex digest must start with `"0000"`, i.e., 16 leading zero bits → ~65,536 expected SHA-256s).
- **Verification**: Server recomputes `SHA256(challenge || nonce)` and checks `strings.HasPrefix(hexdigest, strings.Repeat("0", difficulty))`.

### 3.2 Challenge Flow

Anubis is a **reverse proxy**, not a library. The lifecycle:

1. **First visit**: Anubis evaluates the bot-policy YAML/JSON. If the action is `CHALLENGE`, the request is intercepted and an HTML challenge page is served.
2. **Challenge page** contains embedded constants: `challenge_id`, `difficulty`, `algorithm` ("sha256").
3. **Client solves PoW**: A WebWorker iterates `nonce = 0, 1, 2, ...` computing SHA-256 until the digest has the required leading zero nibbles.
4. **Verification**: Browser POSTs to `/.within.website/x/cmd/anubis/api/pass-challenge` with query params:
   - `id` = challenge ID
   - `nonce` = the found nonce
   - `response` = the hex digest
   - `elapsedTime` = client-side elapsed wall time
   - `redir` = redirect URL
5. **Token issuance**: If valid, a signed JWT cookie (`techaro.lol-anubis-auth`) is set.
6. **Subsequent requests**: Anubis verifies the JWT signature and passes through without re-challenging.

### 3.3 Challenge Issuance API

**`GET /.within.website/x/cmd/anubis/api/make-challenge`** (internal)

Returns JSON:
```json
{
  "rules": {
    "algorithm": "sha256",
    "difficulty": 4
  },
  "challenge": "<hex_challenge_string>",
  "id": "<uuid_challenge_id>"
}
```

Signals used to construct the challenge:
- `Accept-Encoding` header
- `Accept-Language` header  
- `X-Real-IP` (or configured real-IP header)
- Current timestamp
- 19 bytes of cryptographically random data

These are concatenated and SHA-256-hashed to produce the challenge string.

### 3.4 Verification API

**`GET /.within.website/x/cmd/anubis/api/pass-challenge`**

Query parameters:
- `id` — challenge ID (from make-challenge response)
- `response` — hex SHA-256 digest
- `nonce` — integer nonce used
- `elapsedTime` — approximate solve time in ms
- `redir` — redirect target on success

Server verification:
1. Fetches challenge from store (keyed by ID)
2. Recomputes `SHA256(challenge_random_data || nonce)`
3. Checks `strings.HasPrefix(hexdigest, strings.Repeat("0", difficulty))`
4. Validates `elapsedTime` is plausible (anti-cheat heuristic)
5. If valid: mints Ed25519-signed JWT as cookie

### 3.5 JWT Token Format

```json
{
  "challenge": "<challenge_id>",
  "nonce": 12345,
  "response": "<hex_digest>",
  "iat": 1700000000,
  "nbf": 1699999400,
  "exp": 1700604800,
  "method": "sha256",
  "policyRule": "<rule_hash>",
  "action": "challenge",
  "restriction": "<optional_SHA256_of_header>"
}
```

- Signed with **Ed25519** (default) or HS512 (optional)
- Keypair regenerated at each server start
- Cookie default expiry: **7 days** (configurable)
- Cookie attributes: Secure, Partitioned, Domain, Path all configurable
- Cookie bound to policy rule hash — if rules change, clients re-challenge

### 3.6 Difficulty Adjustment

- **Fixed per-policy rule**: Difficulty is set in the bot-policy configuration (YAML/JSON). Default: 4 (zero nibbles).
- Environment variable `DIFFICULTY` can set a global default (1–6).
- No adaptive difficulty; the defense is "make each fresh IP cost something" rather than per-user tuning.
- Anubis also has a **weight system** (since v1.23.0): requests start at weight 10; rules add/subtract weight → final action (ALLOW / CHALLENGE / DENY). But PoW difficulty itself is fixed.

### 3.7 Anti-Replay

- Challenges stored server-side (in-memory Map, Redis, or bbolt) with the `spent` boolean flag
- Challenge store TTL: 30 minutes
- JWT tokens are self-contained (signed) — no per-request server state needed for pass-through
- JWT bound to: `policyRule` hash, optional `restriction` header (e.g., IP hash)
- Tokens cannot be replayed from a different IP if `JWTRestrictionHeader` is configured

### 3.8 Signals Collected (Beyond PoW)

Anubis is notable for its **multi-signal approach**:

1. **Accept-Language** — used in challenge derivation; detects headless browsers with missing/default headers
2. **User-Agent** — evaluated against bot-policy rules (regex matching). Bots get `DENY` or stricter challenge
3. **X-Real-IP** — bound into challenge; prevents challenge sharing across IPs
4. **TLS fingerprinting (JA3/JA4)** — tracked in the issue tracker (#283) as a future enhancement; currently not implemented
5. **elapsedTime** — client-reported solve time; plausibility-checked server-side
6. **Path heuristics**: Challenges not shown for `/.well-known/`, `/robots.txt`, `/favicon.ico`, RSS feeds (`.rss`, `.xml`, `.atom`)
7. **FCrDNS (Forward-Confirmed Reverse DNS)** — optional verification for known bots (e.g., Googlebot)
8. **Rate limiting**: Per-IP token bucket based on configurable thresholds
9. The overall architecture is proxy-level: path, headers, and connection metadata are available naturally

---

## 4. ALTCHA

**URLs**: [altcha.org](https://altcha.org) · [GitHub: altcha-lib](https://github.com/altcha-org/altcha-lib) (TypeScript) · [GitHub: altcha-lib-rs](https://github.com/altcha-org/altcha-lib-rs) (Rust) · [GitHub: altcha-lib-cpp](https://github.com/altcha-org/altcha-lib-cpp) · [GitHub: altcha-lib-py](https://github.com/altcha-org/altcha-lib-py) · [PoW v2 docs](https://altcha.org/docs/v2/proof-of-work-captcha/) · [Rust crate: altcha](https://docs.rs/altcha/latest/altcha/)

### 4.1 PoW Algorithm (v2 — current)

ALTCHA v2 replaces simple hash-matching with a **Key Derivation Function (KDF)** proof-of-work:

- **Algorithm options**:
  - `SHA-256`, `SHA-384`, `SHA-512` — iterated SHA (fast, for testing/low-security)
  - `PBKDF2/SHA-256`, `PBKDF2/SHA-384`, `PBKDF2/SHA-512` — PBKDF2 (recommended default)
  - `SCRYPT` — memory-hard (requires memory_cost)
  - `ARGON2ID` — memory-hard (requires memory_cost + parallelism)
- **What is derived**: `DerivedKey = KDF(Algorithm, Salt, Cost, Password)` where `Password = nonce || counter` (counter as uint32 BE)
- **Difficulty encoding**: The derived key must start with a required hex `keyPrefix` (default: `"00"`). The client brute-forces `counter` values until the derived key's prefix matches.  
- **Deterministic mode**: Server pre-solves at a known counter, includes `keySignature` (HMAC of the derived key), enabling **O(1) verification** without re-deriving the key.

### 4.2 Challenge Format (v2)

JSON payload returned by `createChallenge()` / accepted by the widget:

```json
{
  "algorithm": "PBKDF2/SHA-256",
  "salt": "<random_base64_salt>",
  "nonce": "<random_base64_nonce>",
  "cost": 5000,
  "keyLength": 32,
  "keyPrefix": "00",
  "keySignature": "<optional_HMAC_of_derived_key>",
  "expiresAt": 1700000000,
  "memoryCost": null,
  "parallelism": null,
  "data": null
}
```

Serialized fields (camelCase):
| Field | Type | Description |
|---|---|---|
| `algorithm` | string | KDF algorithm identifier |
| `salt` | string | Random salt (base64 or hex) |
| `nonce` | string | Random nonce (base64 or hex) |
| `cost` | number | Algorithm-specific cost (iterations) |
| `keyLength` | number | Derived key length in bytes (default 32) |
| `keyPrefix` | string | Required hex prefix (default `"00"`) |
| `keySignature` | string | HMAC of derived key (deterministic mode only) |
| `expiresAt` | number | Unix timestamp (seconds) |
| `memoryCost` | number | KiB for memory-hard KDFs |
| `parallelism` | number | For Argon2id |
| `data` | object | Arbitrary metadata |

The whole challenge JSON is **HMAC-signed** and can be transmitted to the client as a base64-encoded string.

### 4.3 Verification

Server-side verification re-derives the key from the submitted counter and checks:
1. HMAC signature of the challenge parameters (tamper protection)
2. `expiresAt` is still in the future
3. `DerivedKey = KDF(params, salt, nonce || counter)` matches the required `keyPrefix`
4. Optionally verifies `keySignature` for deterministic mode fast-path

The `verifySolution()` function returns:
```json
{
  "verified": true,
  "expired": false,
  "invalidSignature": false,
  "invalidSolution": false
}
```

### 4.4 Challenge Binding

- **HMAC signature**: The challenge parameters are signed with `hmacSignatureSecret` (required). The signature is embedded in the challenge string sent to the client.
- **Verification is stateless** (server does not need to store issued challenges). The HMAC secret is all the state needed.
- **Deterministic mode**: A second HMAC secret (`hmacKeySignatureSecret`) signs the derived key directly, enabling O(1) verification by checking the `keySignature` rather than re-deriving the key.
- **TTL**: Optional `expiresAt` field. If set, server checks it during verification.
- **Replay prevention**: Relies on the `store` interface (optional) for tracking used solution counters. Without a store, challenges are replayable within their TTL.

### 4.5 Difficulty Adjustment

- **Cost parameter**: Fixed per-challenge (set by `createChallenge()`). The widget's `createChallengeParameters` callback returns the algorithm and cost.
- ALTCHA does not do dynamic difficulty based on traffic. The operator chooses a cost appropriate for their threat model.
- The switch from SHA-256 to PBKDF2/Argon2id/Scrypt is itself a difficulty lever: memory-hard functions resist GPU/ASIC acceleration.

### 4.6 Anti-Replay

- **Store interface**: ALTCHA provides an optional `Store` interface for tracking used challenge nonces/counters. When a store is provided, `verifySolution()` checks that the solution has not been seen before.
- **TTL**: `expiresAt` bounds the validity window. Without a store, TTL-based expiry is the only protection.
- Replay attacks are prevented only when a store is active.

### 4.7 Signals Collected

- **Pure PoW**: ALTCHA does not collect browser fingerprints or behavioral signals. It is privacy-first by design.
- Optional: The `data` field in the challenge can carry arbitrary metadata (e.g., `{"action": "login"}`) for context, but this is not signal collection.

### 4.8 Rust Support

- **Crate**: [`altcha`](https://docs.rs/altcha/latest/altcha/) (official) and [`altcha-lib-rs`](https://crates.io/crates/altcha-lib-rs) (community)
- Provides: `createChallenge()`, `verifySolution()`, `solveChallenge()` (client-side WASM)
- Supports algorithms: SHA-256/384/512, PBKDF2/SHA-*, SCRYPT, ARGON2ID
- Optional features: `json`, `sha1`

---

## 5. Cloudflare Turnstile

**URLs**: [Cloudflare Turnstile docs](https://developers.cloudflare.com/turnstile/) · [Internals analysis (Crawlex)](https://blog.crawlex.net/blog/cloudflare-turnstile-internals/) · [buchodi.com decryption analysis](https://www.buchodi.com/chatgpt-wont-let-you-type-until-cloudflare-reads-your-react-state-i-decrypted-the-program-that-does-it/) · [ProxyOps 2026 guide](https://proxyops.dev/artiklar/cloudflare-turnstile-how-it-works/)

### 5.1 PoW Algorithm

- **Hash function**: SHA-256 (Hashcash-style)
- **Difficulty**: Uniform random between 400K–500K iterations. 72% of challenges solve in under 5ms (per buchodi's analysis of 100 samples from ChatGPT's PoW).
- The PoW is relatively lightweight. It serves as a **cost-adding layer** and JS-execution proof, not the primary defense.

### 5.2 Architecture (Publicly Known)

Turnstile is **not just PoW**. It is a **multi-layered signal analysis system** with three challenge types:

| Mode | Description |
|---|---|
| **Managed** (recommended) | Selects challenge type based on risk score. Most users see nothing. May escalate to non-interactive or interactive. |
| **Non-interactive** | Always shows a widget but never asks for user interaction. |
| **Invisible** | Fully hidden; challenge runs in background. |

The detection pipeline (inferred):

1. **Network layer**: IP reputation, ASN, routing model, TLS handshake (JA3/JA4), QUIC/HTTP2/HTTP3 behavior, TCP parameters, RTT/jitter
2. **Client layer**: Canvas/WebGL fingerprinting, AudioContext, font enumeration, screen properties, hardware concurrency, device memory
3. **Behavioral layer**: Mouse movement entropy, keystroke timing, scroll patterns, touch events, window focus/blur
4. **Application layer** (ChatGPT-specific): Probes React internals (`__reactRouterContext`, `loaderData`, `clientBootstrap`) to confirm the target application fully hydrated
5. **ML inference**: Ensemble models (gradient boosting, graph networks) produce a risk score
6. **Token issuance**: If low risk → immediate token; borderline → soft challenge; high risk → rejection

### 5.3 The Custom VM / Bytecode System

Per buchodi's decryption analysis (2026), Turnstile uses a **per-request encrypted bytecode program**:

1. Server sends `turnstile.dx` (~28KB base64) in the prepare response
2. XOR-decrypts with a `p` token from the same exchange → 89 VM instructions
3. Those instructions walk a 19KB inner encrypted blob
4. Inner blob decrypted using a **server-generated float key** embedded in the bytecode
5. The actual program is a custom VM with 28 opcodes (ADD, XOR, CALL, BTOA, RESOLVE, JSON_STRINGIFY, etc.) and randomized float register addresses per request
6. **377 programs analyzed** — all checked identical 55 properties across 3 layers (browser, Cloudflare network, React state)

This means **the challenge logic is not a static JS file** — it's dynamically generated per request, making static analysis ineffective.

### 5.4 Challenge Token & Verification

- **Token**: Opaque string up to 2,048 chars, produced client-side after challenge passed
- **Field name**: `cf-turnstile-response` (hidden input)
- **TTL**: 300 seconds (5 minutes)
- **Single-use**: First `siteverify` call consumes it; subsequent calls return `timeout-or-duplicate`

**`POST https://challenges.cloudflare.com/turnstile/v0/siteverify`**

Request:
```json
{
  "secret": "<secret_key>",
  "response": "<token>",
  "remoteip": "<optional_ip>",
  "idempotency_key": "<optional_uuid>"
}
```

Response (success):
```json
{
  "success": true,
  "challenge_ts": "2022-02-28T15:14:30.096Z",
  "hostname": "example.com",
  "error-codes": [],
  "action": "login",
  "cdata": "sessionid-123456789",
  "metadata": {
    "ephemeral_id": "x:9f78e0ed210960d7693b167e"
  }
}
```

- `hostname` — assert matches your domain (cross-domain token replay prevention)
- `action` — echo of widget's action parameter
- `cdata` — echo of widget's cdata parameter (bind to session)
- `metadata.ephemeral_id` — Enterprise-only device fingerprint link

Error codes: `missing-input-secret`, `invalid-input-secret`, `missing-input-response`, `invalid-input-response`, `bad-request`, `timeout-or-duplicate`, `internal-error`.

### 5.5 Privacy Pass Protocol

Turnstile uses the **Privacy Pass** IETF protocol (RFCs 9576, 9474):
- Uses **RSA Blind Signatures** (RFC 9474) to issue tokens without linkability
- Four roles: Origin, Client, Attester, Issuer
- The Attester (Cloudflare) verifies the client passed the challenge
- The Issuer signs a blind token that cannot later be linked to the client
- Standard HTTP auth headers: `WWW-Authenticate: PrivateToken` / `Authorization: PrivateToken`

### 5.6 Signals Collected (Detailed)

| Layer | Signals |
|---|---|
| **TLS/QUIC** | JA3/JA4, cipher suites, extension order, ALPN, 0-RTT behavior, ClientHello characteristics |
| **HTTP/2/3** | Prioritization, frame frequency, HPACK/QPACK behavior, reset frequency |
| **TCP** | Window size, MSS, SACK, SYN/SYN-ACK timing, inter-packet jitter |
| **Network** | IP, ASN, routing model, PTR/rdns, datacenter vs residential, geolocation |
| **Browser** | Canvas, WebGL renderer + vendor, AudioContext, installed fonts, screen dimensions, color depth |
| **Hardware** | `navigator.hardwareConcurrency`, `deviceMemory`, `maxTouchPoints`, `platform`, `vendor` |
| **Automation detection** | `navigator.webdriver`, DevTools flags, WebDriver flags, `window.InstallTrigger` (Firefox) |
| **Behavioral** | Mouse movement (trajectory, velocity, hesitation), keystroke timing, scroll inertia, touch arc, window focus/blur |
| **Application** (ChatGPT) | `__reactRouterContext`, `loaderData`, `clientBootstrap` — confirms React hydration |
| **Storage** | Writes to `localStorage` (key `6f376b6560133c2c`), inspects `quota.estimate` and `usage` |
| **PoW** | SHA-256 hashcash, 400K–500K difficulty, 25 fingerprint fields checked, 7 binary detection flags (`ai`, `createPRNG`, `cache`, `solana`, `dump`, `InstallTrigger`, `data`) |

---

## 6. Synthesis & Best Practices

### 6.1 Consensus Across All Systems

| Aspect | Consensus |
|---|---|
| **Hash function** | SHA-256 is universal choice for PoW. ALTCHA v2 optionally supports memory-hard KDFs. |
| **Difficulty model** | Hashcash-style target prefix (leading zero bits/bytes) or threshold comparison against integer hash value. |
| **Stateless verification** | ALTCHA (with HMAC) and Anubis (with signature) support stateless verification. mCaptcha and Friendly require server-side challenge state. |
| **Verification is mandatory** | All systems require **server-side verification**. Client-side-only checks are trivially bypassed. |
| **Single-use tokens** | Every system enforces single-use tokens. The `timeout-or-duplicate` pattern is universal. |
| **Short TTL** | All systems bound token/challenge lifetime. Common values: 30s (mCaptcha), 5min (Turnstile, Friendly), 10min (ALTCHA), 7 days for proven auth (Anubis). |
| **HMAC signing** | Best practice: HMAC-sign the challenge parameters so the client cannot tamper with difficulty/cost. ALTCHA and Friendly do this; mCaptcha relies on server-side challenge state instead. |
| **Salt per instance** | Every system uses a unique salt to prevent cross-system PoW reuse. |
| **Privacy-first option** | mCaptcha, ALTCHA, and Friendly lite-server are pure PoW with no fingerprinting. This is a strong differentiator vs Turnstile. |

### 6.2 Common Pitfalls

| Pitfall | How It Manifests | Prevention |
|---|---|---|
| **Challenge replay** | Attacker reuses a solved challenge across multiple requests | Single-use tracking + short TTL + HMAC signature binding challenge to nonce+context |
| **Difficulty spoofing** | Client modifies the difficulty byte/parameter to make PoW trivially easy | HMAC-sign the entire challenge parameters; verify signature before checking PoW |
| **Missing HMAC** | Without signing, any proxy or client can tamper with challenge parameters | Always HMAC-sign challenges with a server-held secret |
| **Weak RNG** | Predictable nonces or salts enable PoW precomputation | Use `crypto.randomBytes()` or equivalent CSPRNG for all nonces and salts |
| **No replay tracking** | Stolen tokens can be reused until they expire | Track spent token nonces; use single-use semantics |
| **Client-side-only verification** | Widget output is trusted without server verification | Always validate server-side before acting on the token |
| **Race conditions on verification** | Concurrent requests may both pass if token is not atomically spent | Use idempotency keys (Turnstile pattern) or atomic compare-and-swap on token state |
| **IP binding** | Token solved on IP A is used from IP B | Bind challenge to client IP or session identifier |
| **Cross-domain token reuse** | Token from one site replayed against another | Embed origin/hostname in the challenge and verify server-side |
| **Pre-computation attacks** | Known salt + known challenge format allows precomputing valid solutions | Use per-request random nonces, short TTLs, and rotate salts regularly |

### 6.3 Architectural Decisions for Your Rust + WASM + TypeScript Implementation

**Recommended approach — layered PoW with optional signals:**

```
Layer 1: Pure PoW (always on, minimal friction)
  ↳ SHA-256 Hashcash with configurable difficulty
  ↳ HMAC-signed challenges (stateless verification with server secret)
  ↳ Adaptive difficulty targeting ~0.5–3s client solve time

Layer 2: PoW + Signals (optional escalation)
  ↳ Collect lightweight browser signals (WebGL, canvas, fonts)
  ↳ JS execution proof (mandatory: client must execute WASM)
  ↳ TLS/JA3 fingerprint at the edge
  ↳ IP reputation scoring

Layer 3: Full challenge (defense in depth)
  ↳ Interactive fallback (I'm not a robot checkbox)
  ↳ Behavioral analysis (mouse movement, keystroke timing)
```

---

## 7. Recommended Protocol Schema

Based on the best ideas from all five systems, here is a concrete protocol for `/challenge` and `/verify` endpoints:

### 7.1 `POST /api/v1/challenge` — Issue a PoW Challenge

**Request**:
```json
{
  "sitekey": "string (public identifier)"
}
```

**Response** (200):
```json
{
  "challenge": {
    "algorithm": "SHA-256",
    "salt": "b64(32 random bytes)",
    "nonce": "b64(16 random bytes)",
    "difficulty": 4,
    "expires_at": 1700000000,
    "signature": "b64(HMAC-SHA256(challenge_params, server_secret))"
  }
}
```

- `algorithm`: `"SHA-256"` for standard PoW. Future: `"PBKDF2/SHA-256"`, `"ARGON2ID"`.
- `difficulty`: Leading zero **nibbles** required (4 = 16 bits, ~65K avg hashes).
- `salt`: Per-instance fixed salt (long, random, rotated periodically).
- `nonce`: Per-request random nonce (prevents challenge precomputation).
- `expires_at`: Unix timestamp (seconds). Default: 120s from now.
- `signature`: HMAC-SHA256 over the serialized challenge JSON (sorted keys). Prevents client from tampering with `difficulty`, `expires_at`, etc.

**Optional fields for extended mode**:
- `mode`: `"pow" | "pow+signals"` — the widget mode to initialize
- `signals_config`: object describing which signals to collect (if any)
- `origin`: the expected request origin (defaults to `Origin` header)

### 7.2 `POST /api/v1/verify` — Verify a PoW Solution

**Request**:
```json
{
  "sitekey": "string",
  "challenge_nonce": "string (from challenge response)",
  "counter": 123456,
  "digest": "hex(SHA-256(salt || challenge_nonce || counter))",
  "signals": {},
  "signals_hmac": "optional"
}
```

- `counter`: The nonce/increment found by the client where `digest` has the required leading zero bits.
- `digest`: The full SHA-256 hex digest. Server recomputes and verifies.
- `signals`: Optional JSON object with any collected browser signals.
- `signals_hmac`: Optional HMAC of signals (prevent tampering).

**Response** (200) — success:
```json
{
  "success": true,
  "token": "string (signed JWT or opaque token for subsequent auth)",
  "expires_at": 1700000500
}
```

**Response** (200) — failure:
```json
{
  "success": false,
  "error": {
    "code": "solution_invalid | expired | duplicate | tampered",
    "detail": "Human-readable description"
  }
}
```

### 7.3 Server Verification Logic

```
1. Verify HMAC signature on challenge parameters (reject if tampered)
2. Check expires_at against current time (reject if expired)
3. Look up challenge_nonce in replay cache (reject if duplicate; then store)
4. Recompute digest = SHA-256(salt || challenge_nonce || counter)
5. Check digest has required leading zero bits/nibbles
6. (Optional) Verify signals HMAC and process signal payload
7. If valid: issue signed token, mark challenge_nonce as spent
```

### 7.4 Rust Data Structures (Suggested)

```rust
// Server-side challenge creation
pub struct ChallengeParams {
    pub salt: Vec<u8>,
    pub nonce: [u8; 16],
    pub difficulty: u8,       // leading zero nibbles
    pub algorithm: String,
    pub expires_at: u64,      // unix seconds
}

pub struct ChallengeResponse {
    pub algorithm: String,
    pub salt: String,         // base64
    pub nonce: String,        // base64
    pub difficulty: u8,
    pub expires_at: u64,
    pub signature: String,    // base64(hmac)
}

// Client submits
pub struct VerifyRequest {
    pub sitekey: String,
    pub challenge_nonce: String,
    pub counter: u64,
    pub digest: String,       // hex(SHA-256)
    pub signals: Option<serde_json::Value>,
    pub signals_hmac: Option<String>,
}

// Server responds
pub struct VerifyResponse {
    pub success: bool,
    pub token: Option<String>,
    pub expires_at: Option<u64>,
    pub error: Option<VerifyError>,
}

pub struct VerifyError {
    pub code: String,
    pub detail: String,
}

// Token (JWT claims)
pub struct TokenClaims {
    pub sub: String,           // sitekey
    pub jti: String,           // unique token ID
    pub iat: u64,
    pub exp: u64,
    pub nonce: String,         // bound to challenge
    pub ip_hash: Option<String>,
}
```

### 7.5 Difficulty Tuning Strategy

Use **adaptive difficulty** based on rolling average solve time:

```
Target: 1.0 seconds client-side solve time

Measure: rolling median of reported solve times (from client diagnostics)
Adjust: if solve_time < 0.5s → difficulty += 1
        if solve_time > 3.0s → difficulty -= 1
        Clamp: [2, 12] (4 nibbles = 16 bits through 12 nibbles = 48 bits)

Default start: difficulty = 4 (~65K hashes, ~50-200ms desktop, ~500ms mobile)
```

When under DoS attack (detected via request rate per IP/sitekey), increase a **site-wide multiplier** (×2, ×4, ×8) to the difficulty.

### 7.6 Anti-Replay Storage Strategy

Two-tier approach:

1. **Challenge nonces**: Cache with TTL (120s). Key: `spent:<nonce>`. Value: timestamp. Evict after expiry. (Redis, or in-memory with periodic GC.)
2. **Tokens**: Self-contained JWT (signed with Ed25519). No server-side token storage needed. Single-use enforced by verifying token `jti` against a spent-token cache with TTL matching token expiry.

This means **token verification is stateless** unless the token has been spent, matching the best of ALTCHA's HMAC + Anubis's JWT pattern.

### 7.7 WASM Client Architecture

```
┌──────────────────────────────┐
│  TypeScript Widget            │
│  ┌──────────────────────────┐ │
│  │ Challenge Fetcher        │ │ ← fetch /api/v1/challenge
│  │  └─ parse & verify HMAC  │ │
│  ├──────────────────────────┤ │
│  │ PoW Solver (WASM)        │ │ ← rust->wasm-pack
│  │  └─ SHA-256 loop         │ │
│  │  └─ progress callback    │ │
│  ├──────────────────────────┤ │
│  │ Signal Collector (opt)   │ │ ← WebGL, canvas, timing
│  ├──────────────────────────┤ │
│  │ Token Submitter          │ │ ← POST /api/v1/verify
│  └──────────────────────────┘ │
└──────────────────────────────┘
```

- WASM solver compiles Rust SHA-256 via `wasm-pack`. Provides `solve_challenge(challenge_params) -> (counter, digest)`.
- JS wrapper fetches challenge, spawns Web Worker with WASM, monitors solve progress, submits result.
- All cryptographic operations (HMAC verification of challenge) happen on the server; the client never holds the HMAC secret.

---

## References

### Friendly Captcha
- [friendly-pow README](https://github.com/friendlycaptcha/friendly-pow#readme) — Puzzle format, difficulty formula, solution format
- [friendly-challenge widget](https://github.com/FriendlyCaptcha/friendly-challenge) — Client SDK, puzzle endpoint
- [friendly-docs (siteverify v2)](https://developer.friendlycaptcha.com/docs/v2/api/siteverify) — API reference
- [friendly-docs (verify)](https://developer.friendlycaptcha.com/docs/v2/getting-started/verify) — Verification walkthrough
- [friendly-lite-server](https://github.com/FriendlyCaptcha/friendly-lite-server) — Self-hosted reference server
- [friendly-captcha-go](https://github.com/FriendlyCaptcha/friendly-captcha-go) — Go SDK (wire.go for request types)

### mCaptcha
- [mCaptcha README](https://github.com/mCaptcha/mCaptcha#readme) — Architecture overview
- [pow_sha256 Rust crate](https://docs.rs/mcaptcha_pow_sha256) — PoW algorithm spec
- [pow_wasm](https://mcaptcha.github.io/pow_wasm/) — WASM solver API
- [Configuration](https://github.com/mCaptcha/mCaptcha/blob/master/docs/CONFIGURATION.md) — Difficulty adjustment strategy
- [ACM Paper (2024)](https://cacm.acm.org/research/mcaptcha-replacing-captchas-with-rate-limiters-to-improve-security-and-accessibility/) — Peer-reviewed design paper
- [API context7 docs](https://context7.com/mcaptcha/mcaptcha/llms.txt) — `/api/v1/pow/config` endpoint spec

### Anubis
- [How Anubis Works](https://github.com/TecharoHQ/anubis/blob/main/docs/docs/design/how-anubis-works.mdx) — Challenge flow, JWT format
- [Why Proof of Work](https://github.com/TecharoHQ/anubis/blob/main/docs/docs/design/why-proof-of-work.mdx) — Design rationale
- [API docs](https://techarohq-anubis.mintlify.app/api/anubis) — make-challenge, pass-challenge endpoints
- [Source: lib/anubis.go](https://github.com/TecharoHQ/anubis/blob/61682e49/lib/anubis.go) — Challenge issue/verify/cookie logic
- [Hivebook summary](https://www.hivebook.wiki/wiki/anubis-v1250-sha-256-proof-of-work-ai-scraper-firewall) — v1.25.0 architecture details

### ALTCHA
- [PoW v2 docs](https://altcha.org/docs/v2/proof-of-work-captcha/) — Algorithm, challenge format
- [altcha-lib TypeScript](https://github.com/altcha-org/altcha-lib) — createChallenge, verifySolution API
- [altcha-lib-rs Rust](https://github.com/altcha-org/altcha-lib-rs) — Rust implementation
- [altcha-lib-py](https://github.com/altcha-org/altcha-lib-py) — Python reference (PoW v2 spec)
- [altcha-lib-cpp](https://github.com/altcha-org/altcha-lib-cpp) — C++ reference
- [docs.rs: altcha](https://docs.rs/altcha/latest/altcha/) — Rust crate docs
- [ChallengeParameters schema](https://hexdocs.pm/altcha/Altcha.V2.ChallengeParameters.html) — Elixir hexdocs (canonical field docs)

### Cloudflare Turnstile
- [Turnstile overview](https://developers.cloudflare.com/turnstile/) — Official docs
- [Server-side validation](https://developers.cloudflare.com/turnstile/get-started/server-side-validation/) — siteverify API
- [Turnstile internals (Crawlex, 2026)](https://blog.crawlex.net/blog/cloudflare-turnstile-internals/) — Token lifecycle, API format, siteverify response, Privacy Pass
- [Buchodi decryption analysis (2026)](https://www.buchodi.com/chatgpt-wont-let-you-type-until-cloudflare-reads-your-react-state-i-decrypted-the-program-that-does-it/) — Custom VM, 55 properties, bytecode encryption, React state probing, PoW analysis
- [ProxyOps 2026 guide](https://proxyops.dev/artiklar/cloudflare-turnstile-how-it-works/) — Signal categories, Private Access Tokens
- [MobileProxy.rent 2026](https://mobileproxy.rent/en/pages/cloudflare-turnstile-in-2026-architecture-signals-behavior-and-mobile-ip-role.html) — Behavioral analysis, TLS/QUIC, CGNAT, ML inference
- [Cloudflare docs: Token validation](https://developers.cloudflare.com/turnstile/turnstile-analytics/token-validation/) — Error codes, token expiry
- [Cloudflare docs: Testing](https://developers.cloudflare.com/turnstile/troubleshooting/testing/) — Response format with dummy tokens
- [jguillaumesio blog (2026)](https://jguillaumesio.com/blog/cloudflare-under-the-hood/) — Edge network, bot score 1-99
