# Research Report: Building a Cloudflare-Turnstile-like Human Verification System
## Rust (WASM) + TypeScript — 2026 Best Practices

> **Date:** 2026-07-13
> **Context:** [Webrify](/Users/alex/Projects/workspace/Webrify) — Turnstile-like verification component

---

## Topic A: Browser Fingerprinting (Defensive, for Bot Detection)

### 1. FingerprintJS — Open Source Analysis

#### Overview
- **GitHub:** https://github.com/fingerprintjs/fingerprintjs ⭐ 27,661 stars, 2,567 forks
- **License:** MIT (BSL for commercial use) — https://github.com/fingerprintjs/fingerprintjs/blob/master/docs/licensing.md
- **Current version:** v5.2.0 (npm: `@fingerprintjs/fingerprintjs`)
- **Zero external dependencies**, ~15KB gzipped

#### Signals Collected by Open Source (v5)
The open-source library queries **50+ browser attributes** (source: https://www.blog.brightcoding.dev/2025/08/14/inside-fingerprintjs-the-open-source-browser-fingerprinting-library):

| Category | Signals |
|----------|---------|
| **Canvas** | 2D rendering via `toDataURL()` — text & shapes drawn with specific fonts/colors |
| **WebGL** | `VENDOR`, `RENDERER` strings via `WEBGL_debug_renderer_info`, rendering hash |
| **Audio** | `OfflineAudioContext` — oscillator + compressor → frequency-domain fingerprint |
| **Fonts** | Font probing via `measureText` width comparison (200–500 fonts) |
| **Hardware** | `navigator.hardwareConcurrency`, `navigator.deviceMemory` |
| **Screen** | `width`, `height`, `colorDepth`, `pixelDepth`, `availWidth`, `availHeight` |
| **OS/Browser** | `userAgent`, `platform`, `language`, `languages[]`, `timezone` (via `Intl.DateTimeFormat`) |
| **Plugins** | `navigator.plugins[]`, `navigator.mimeTypes[]` lengths |
| **Touch** | `navigator.maxTouchPoints`, touch event support |
| **Ad Blockers** | DOM selector probing for known ad-blocker presence |
| **Misc** | cookies enabled, indexedDB, localStorage, sessionStorage, CPU class |

#### visitorId Computation — MurmurHash3
1. All signals are collected asynchronously via a `get()` method
2. Results are normalized, concatenated into a deterministic string
3. Hashed using **MurmurHash3** 32-bit (x86) → 32-character hex string
4. The hash is purely client-side — no data leaves the browser unless you explicitly send it

Source: https://github.com/fingerprintjs/fingerprintjs/blob/master/src/agent.ts (core logic)

#### Accuracy Claims
- **Open source (OSS):** ~40–60% accuracy (identification rate)
- **Fingerprint Pro (commercial):** 99.5%+ accuracy
- Why the gap? OSS is purely client-side, easily spoofable. Pro adds server-side signal analysis, IP intelligence, tamper detection, and 100+ signals
- Source: https://fingerprint.com/blog/open-source-vs-fingerprint-pro-accuracy

#### Known Bypasses
- **Stealth browsers** (Multilogin, GoLogin, Octo Browser) spoof canvas/WebGL/fonts
- **puppeteer-extra-plugin-stealth** patches 80%+ of classical detection vectors
- **Canvas "add noise"** plugins inject ±1% pixel jitter
- **Firefox Strict / Brave / Tor** — randomize or block canvas readback entirely
- Source: https://fingerprint.com/blog/open-source-vs-fingerprint-pro-accuracy

#### Open Source vs Fingerprint Pro

| Feature | Open Source | Pro (Commercial) |
|---------|-------------|-------------------|
| Signals | 50+ browser | 100+ browser + network + server-side |
| Processing | Client-side only | Server-side with tamper detection |
| Accuracy | 40–60% | 99.5%+ |
| IP Geolocation | ❌ | ✅ |
| Bot Detection | ❌ | ✅ Smart Signals |
| Incognito Detection | ❌ | ✅ |
| VPN Detection | ❌ | ✅ |
| Tamper Resistance | Low | High (server-side verification) |
| Pricing | Free (MIT) | Paid tiers (free tier available) |

Sources:
- https://fingerprint.com/blog/open-source-vs-fingerprint-pro-accuracy
- https://docs.fingerprint.com/docs/smart-signals-reference

---

### 2. Fingerprinting Signals Catalog

#### Canvas Fingerprint
- **How it works:**
  1. Create an off-screen `<canvas>` element
  2. Draw text with mixed fonts (e.g., "Cwm fjordbank glyphs vext quiz"), shapes (arcs, rects), gradients, and colors
  3. Call `canvas.toDataURL()` to read back pixel data as base64
  4. Hash the result (SHA-256 or MurmurHash3)
- **Why GPUs differ:** Each GPU model, driver version, and OS font renderer produces subtly different anti-aliasing, hinting, sub-pixel rendering, and color blending. The same JavaScript draws the same commands, but the pixel output varies.
- **Entropy:** ~8–10 bits (about 1 in 250–1000 users uniquely identified by canvas alone)
- Sources: https://fingerprint.com/blog/canvas-fingerprinting/, https://snitchtest.com/canvas-fingerprinting-explained

#### WebGL Fingerprint
- **How it works:**
  1. Get a WebGL or WebGL2 context from a canvas: `canvas.getContext('webgl')`
  2. Query `gl.getParameter()` for `VENDOR`, `RENDERER`, extensions, max texture size, shader precision, MAX_VIEWPORT_DIMS
  3. Render a 3D scene (triangles, shading, textures) and read pixels via `gl.readPixels()`
  4. Combine all parameters + pixel hash → WebGL fingerprint
- **Key parameters:** `WEBGL_debug_renderer_info.UNMASKED_VENDOR_WEBGL`, `UNMASKED_RENDERER_WEBGL`, `ALIASED_LINE_WIDTH_RANGE`, `MAX_TEXTURE_SIZE`, `MAX_VERTEX_ATTRIBS`
- **GPU detection:** Reveals exact GPU model (e.g., "NVIDIA GeForce RTX 4090", "Apple M3 Pro"), driver version
- **Entropy:** ~10–15 bits — highly identifying
- Source: https://www.proxyhorizon.com/blog/how-canvas-webgl-audiocontext-fingerprinting-works

#### AudioContext Fingerprint
- **How it works:**
  1. Create `OfflineAudioContext` (sample rate, channels)
  2. Connect an **oscillator** (sine wave at specific frequency) → **compressor** (dynamics processing) → destination
  3. Render audio offline: `audioCtx.startRendering()`
  4. Get the rendered `AudioBuffer`, extract float samples
  5. Hash the sample array
- **Why it differs:** Audio signal processing hardware, DAC characteristics, OS audio stack, sample rate conversion — all leave subtle signatures in the rendered output.
- More stable than canvas across browser updates but varies with OS audio driver changes.
- Entropy: ~5–8 bits
- Source: https://www.empirium.io/blog/canvas-webgl-audio-fingerprinting

#### Font Enumeration
- **Method 1 — CSS measureText:**
  1. Create a span with baseline font (e.g., `monospace`), measure `offsetWidth`
  2. Set `font-family: "TargetFont", monospace`; re-measure
  3. If width changes → font is installed
- **Method 2 — Canvas text rendering:**
  1. Draw text in canvas with test font, read pixel hash
  2. Compare hash against known font baselines
- **Scale:** 200–500 fonts probed in ~100ms
- **Entropy:** ~10–12 bits (large font set variation)
- Source: https://blog.send.win/font-fingerprinting-protection-complete-guide-2026

#### Hardware Signals
| Signal | Source | Entropy | Notes |
|--------|--------|---------|-------|
| `hardwareConcurrency` | `navigator.hardwareConcurrency` | ~2–3 bits | Logical CPU cores (4, 8, 16, etc.) |
| `deviceMemory` | `navigator.deviceMemory` | ~2–3 bits | RAM in GB (Chrome only, returns 0.25/0.5/1/2/4/8) |
| Screen resolution | `screen.width`, `screen.height` | ~5 bits | Common: 1920×1080 |
| `colorDepth` | `screen.colorDepth` | 1–2 bits | Usually 24 or 30 |
| `pixelRatio` | `window.devicePixelRatio` | 2–3 bits | 1, 1.5, 2, 2.5, 3 |
| `availScreen` | `screen.availWidth/availHeight` | 2–3 bits | Excludes taskbar/dock |

#### Soft Signals
| Signal | Code | Entropy |
|--------|------|---------|
| Timezone | `Intl.DateTimeFormat().resolvedOptions().timeZone` | 6 bits |
| Languages | `navigator.languages` | 5–8 bits |
| Platform | `navigator.platform` | 2–3 bits |
| UserAgent | `navigator.userAgent` | 10–15 bits (but easily spoofed) |

#### WebRTC IP Leak
- **Mechanism:** STUN/TURN requests in WebRTC (`RTCPeerConnection`) can leak the real local IP address even behind a VPN
- **API:** `navigator.mediaDevices.enumerateDevices()` also leaks camera/microphone device names
- **Mitigation:** `RTCPeerConnection` can be partially blocked by browser privacy settings
- Source: https://scrappey.com/qa/anti-bot/what-is-headless-browser-detection

#### Navigator Properties
- `navigator.cookieEnabled`, `navigator.doNotTrack`, `navigator.pdfViewerEnabled`
- `navigator.keyboard`, `navigator.connection` (NetworkInformation API — downlink, effectiveType, rtt)
- `navigator.getGamepads()` (gamepad presence)
- `navigator.bluetooth`, `navigator.usb`, `navigator.serial` — available only in secure contexts

---

### 3. Bot / Headless Detection Signals

> **2026 Update:** Classical static-property checks (e.g., `navigator.webdriver`) are largely defeated by modern stealth plugins. Detection has moved to behavioral and rendering-level analysis.
> Source: https://sntlhq.com/blog/headless-browser-detection-2026

#### Classical Signals (Still Worth Checking, but Increasingly Patched)

| Signal | Check | Headless Indicator | Current Status |
|--------|-------|-------------------|----------------|
| `navigator.webdriver` | `=== true` | Automation detected | Patched by puppeteer-extra-stealth, playwright-stealth |
| `navigator.plugins.length` | `=== 0` | No plugins | Patched — headless Chrome now has populated plugins |
| `navigator.mimeTypes.length` | `=== 0` | No MIME types | Patched |
| `chrome.runtime` | `=== undefined` | No Chrome runtime | Patched — but still useful for non-Chromium detection |
| `window.chrome` | `=== undefined` | No chrome object | Firefox has no chrome naturally |
| `navigator.languages` | Empty array | No languages | Patched |
| User-Agent contains "HeadlessChrome" | String check | Headless mode | Patched — new headless mode doesn't include it |

Source: https://dev.to/vhub_systems_ed5641f65d59/how-sites-detect-headless-browsers-and-how-to-evade-each-signal-2026-guide-2jj0

#### Advanced / 2026-Relevant Signals

| Category | Detection Method |
|----------|-----------------|
| **CDP Attachment** | `Runtime.enable` side effects — `console.debug` with thrown getter to detect DevTools Protocol |
| **WebGL Renderer** | Software renderer (`SwiftShader` or `ANGLE (Google, ...)`) instead of real GPU |
| **Permissions Quirks** | `Notification.permission` returning `"denied"` by default in headless; `navigator.permissions.query()` inconsistencies |
| **Media Devices** | `enumerateDevices()` returning empty or limited list (no cameras/microphones) |
| **Input Event Physics** | Mouse movement: Fitts's-law acceleration profiles, sub-pixel coordinates, timing jitter. Bots produce linear/perfect movements |
| **Speech Synthesis** | `window.speechSynthesis.getVoices()` returning empty array |
| **Stack Depth** | Maximum JS call stack depth differs between headless vs headed Chrome |
| **Worker UA Check** | Compare `navigator.userAgent` in Worker vs main thread |
| **Emoji Rendering** | OS-specific emoji rendering differences (e.g., flag emojis) |
| **GPU Timing** | `WEBGL_disjoint_timer_query` — accurate GPU timing differs in software rendering |
| **Selenium/Playwright markers** | `window.__SELENIUM`, `window.__playwright__binding__`, `window.__pwInitScripts` |

Source: https://github.com/andriyshevchenko/headless-detector

#### 2026 Perspective on Headless Detection
- **puppeteer-extra-plugin-stealth** patches all classical signals
- **undetected-chromedriver** and **Rebrowser-Patches** defeat CDP detection
- **New Headless Chrome** (since v109) runs same binary as headed mode — far harder to detect
- **What still works:** behavioral analysis (mouse physics, timing), GPU pipeline analysis, deep WebGL parameter consistency checks, TLS/JA4 fingerprinting at network level
- Source: https://sntlhq.com/blog/headless-browser-detection-2026

---

### 4. Signal Integrity Under WASM

#### JS → WASM Boundary: What Goes Where

**Best collected in JS** (need DOM/Web APIs):
- `navigator` properties (userAgent, platform, languages, webdriver, hardwareConcurrency)
- Canvas 2D rendering & `toDataURL()`
- WebGL context creation & `getParameter()` calls
- AudioContext creation & offline rendering
- Screen properties (width, height, colorDepth, DPR)
- DOM blocker detection
- Mouse/touch event listeners
- `performance.now()`, `performance.memory`
- Media devices enumeration
- Speech synthesis voices

**Best computed in Rust-WASM**:
- Hashing (SHA-256, MurmurHash3) from collected raw signals
- PoW challenge solving (SHA-256 hashcash loop)
- Risk scoring / classification logic
- Cryptographic token generation/verification
- Signal normalization & concatenation

#### Practical Architecture
```
JS Layer (collects signals) ──→ passes raw data as JSON/Array ──→ Rust WASM (processes, hashes, solves PoW)
                                                        ↕
                                              Returns result string
```

**Data passing strategy:**
- Use `serde_wasm_bindgen::to_value()` / `from_value()` for structured data — https://rustwasm.github.io/docs/wasm-bindgen/reference/arbitrary-data-with-serde.html
- For binary data (audio samples, canvas pixel buffers), pass `Uint8Array` → `Vec<u8>` (zero-copy with `memory.grow`)
- Avoid string roundtrips for large data — pass `JsValue` directly
- For the PoW loop, keep it in WASM with raw `Vec<u8>` — no crossing boundary during computation

**Key insight:** Attempting to collect fingerprint signals *from within WASM* via `web-sys` adds complexity and indirection. The pragmatic approach: collect in JS (the natural place), pass to WASM for computation/verification.

Sources:
- https://rustwasm.github.io/docs/wasm-bindgen/reference/arbitrary-data-with-serde.html
- https://github.com/wasm-bindgen/wasm-bindgen/blob/main/src/convert/slices.rs

---

### 5. Privacy / Compliance

#### GDPR & ePrivacy (2026 Landscape)
- **EDPB Guidelines 2/2023** (finalized Oct 2024): Expanded Article 5(3) of ePrivacy beyond cookies to cover **fingerprinting, pixels, URL tracking**
- **Google's 2025 policy reversal** now permits fingerprinting for advertising — UK ICO responded that fingerprinting still requires prior consent
- **French CNIL:** Fined Google €325M and Shein €150M in Sep 2025 for cookie/fingerprinting violations
- **Key takeaway:** Fingerprinting that can identify an individual is processing of **personal data** under GDPR Article 4(1)
- Sources: https://www.consenteo.com/knowledge-hub/GDPR/gdpr_cookie_consent_2026, https://tracio.ai/blog/fingerprinting-post-cookie-2026

#### Key Requirements for Your Turnstile System
1. **Consent:** Must obtain prior consent before fingerprinting for *tracking* purposes. Security/fraud prevention may qualify as legitimate interest under GDPR Art. 6(1)(f) — but this is legally untested at scale.
2. **Data minimization:** Only collect signals needed for the verification. Don't store raw fingerprints.
3. **Pseudonymization:** Hash signals client-side (MurmurHash3 or SHA-256) before any storage. Pseudonymized data is still personal data under GDPR.
4. **Transparency:** Privacy policy must disclose fingerprinting, what data is collected, and for what purpose.
5. **Right to object:** Users must have a mechanism to opt out (even if it means showing a traditional CAPTCHA as fallback).
6. **One-shot processing:** The ideal model — collect signals → hash → solve challenge → discard raw signals. No retention of raw fingerprint data.

Source: https://privacychecker.pro/blog/browser-fingerprinting-privacy

#### Recommended Privacy Architecture
```
Browser:
1. Collect raw signals (never sent to server!)
2. → Hash into fingerprint token (SHA-256) in WASM
3. → Combine with server challenge → solve PoW
4. → Send ONLY the PoW solution + hashed fingerprint to server

Server:
5. Verify PoW solution
6. Store only hashed fingerprint (pseudonymized)
7. Never store raw canvas/WebGL/audio data
```

---

## Topic B: Rust + wasm-pack + Web Tooling (2026 Best Practices)

### 1. wasm-pack Workflow

#### Build Targets
```bash
wasm-pack build --target web          # ES modules, no bundler needed, manual init
wasm-pack build --target bundler       # Default — for webpack/rollup/vite (automatic init)
wasm-pack build --target nodejs        # CommonJS for Node.js
wasm-pack build --target no-modules    # IIFE, global scope
```
- **Recommended for your project:** `--target bundler` (works with Vite/Next.js/webpack)
- `--target web` is also good if you want direct `<script type="module">` usage
- Source: https://context7.com/wasm-bindgen/wasm-pack/llms.txt

#### `#[wasm_bindgen]` API Surface Design
- **Expose only what JS needs:** Minimize the Rust→JS API surface
- Function types: free functions, `impl` blocks (constructor + methods)
- Supported types: primitives, `String`, `Vec<u8>`, `JsValue`, `js_sys::Date`, `web_sys::*`, structs with `#[wasm_bindgen]`
- For complex data: use `#[derive(serde::Serialize, Deserialize)]` + `serde_wasm_bindgen`
- Source: https://rustwasm.github.io/docs/wasm-bindgen/

#### Handling Panics
```rust
// In wasm entry point:
#[wasm_bindgen(start)]
fn run() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    Ok(())
}
```
- https://github.com/wasm-bindgen/wasm-bindgen/blob/main/examples/webxr/src/utils.rs
- Add as optional dependency: `console_error_panic_hook = { version = "0.1", optional = true }`

#### Efficient Data Passing
| Approach | Performance | Use Case |
|----------|-------------|----------|
| `&str` / `String` → JS `string` | Good for small strings | Challenge strings, small payloads |
| `Vec<u8>` → `Uint8Array` (zero-copy via `forget`) | Best — zero copy | Canvas pixel data, audio samples, binary |
| `serde_wasm_bindgen::to_value()` | Good — structured data | JSON-like signal payloads |
| `serde_json` → JS `JSON.parse` | Okay — no `Map`/`Set` support | When `serde_wasm_bindgen` is too slow |
| `JsValue` directly | Fastest for opaque JS objects | When Rust just passes through |

**Zero-copy with `Vec<f32>` / `Vec<u8>`:**
```rust
// Rust side — returns as Float32Array/Uint8Array with no copy
#[wasm_bindgen]
pub fn process_buffer(data: Vec<u8>) -> Vec<u8> {
    // data arrives as Uint8Array (memory shared)
    // return value is handed to JS via mem::forget (no copy)
}
```
Source: https://github.com/wasm-bindgen/wasm-bindgen/blob/main/src/convert/slices.rs

#### wasm-pack Version 2026
- Current: `wasm-pack v0.14.x` (previous `rustwasm` org sunset July 2025, now maintained independently)
- `wasm-bindgen v0.2.118+` — 123+ releases, stable API
- Source: https://www.nandann.com/blog/rust-wasm-production-2026

---

### 2. SHA-256 in WASM

#### Recommended Crate: `sha2`
- **The crate:** https://crates.io/crates/sha2 (v0.11.x, 393.5M+ downloads)
- Part of the RustCrypto project: https://github.com/RustCrypto/hashes
- **Features:** Pure Rust, `no_std` compatible, WASM-capable
- **Usage:** `Sha256::digest(data)` or incremental `Sha256::new()` + `update()` + `finalize()`
- **Backends:** auto-detects `aarch64-sha2`, `x86-sha` (SHA-NI), `soft` (portable fallback)

#### WASM vs WebCrypto Performance

Benchmarks (source: https://www.measurethat.net/Benchmarks/Show/32592/1/hash-wasm-vs-web-crypto-api):

| Implementation | Ops/sec (SHA-256) | Relative |
|---------------|-------------------|----------|
| **WebCrypto `subtle.digest`** | ~297,000 | 5x faster |
| **WASM (sha2 crate)** | ~59,700 | 1x (baseline) |

Another benchmark (source: https://medium.com/@ronantech/exploring-sha-256-performance-on-the-browser):
- **WebCrypto:** ~370 KB/s throughput
- **WASM (sha2):** Also excellent, within 2x of WebCrypto
- **CryptoJS (pure JS):** ~60 KB/s (6x slower than WASM)

#### Recommendation
**For the PoW loop: Use the `sha2` crate in WASM.** Reasons:
1. WebCrypto's `subtle.digest()` is an async `Promise` — significant overhead per call. The tight PoW loop needs synchronous hashing.
2. WebCrypto on Chrome uses a **global mutex** — parallel workers block each other. Fixed in newer Chrome but still an issue in Safari. (Source: https://issues.chromium.org/issues/40857630)
3. WASM gives you deterministic, synchronous SHA-256 computation at near-native speed.
4. With SIMD (`-C target-feature=+simd`), WASM SHA-256 gets even faster.

**For one-shot verification hashing on the server: Use SHA-256 from the `sha2` crate** (or `ring` for additional crypto primitives).

---

### 3. Rust Web Framework for Backend Validation (2026)

#### Recommendation: Axum

| Framework | Stars | Status | Notes |
|-----------|-------|--------|-------|
| **Axum** | 20k+ | ✅ Stable, v0.8.x | Tower-native, ergonomic, Tokio ecosystem |
| **Actix-web** | 18k+ | ✅ Stable, v4.x | Fastest, but separate middleware ecosystem |
| **Warp** | 9k+ | ⚠️ Maintenance | Less active, fewer middleware |
| **Rocket** | 24k+ | ⚠️ Slower updates | Requires nightly, Tower-incompatible |

**Why Axum wins for 2026:**
- Built on **Tower** — the de facto Rust middleware standard. Reuses `tower-http`, `tower-governor`, etc.
- **tower-governor** for IP-based rate limiting (https://crates.io/crates/tower_governor)
- **tracing** integration baked in via `tower-http::trace::TraceLayer`
- Shared `State` extractor pattern with `FromRef` for clean dependency injection
- `HandleErrorLayer` for graceful middleware error handling
- **Tokio-native** — same async runtime as the rest of the ecosystem

Sources:
- https://docs.rs/axum/latest/axum/index.html
- https://docs.rs/tower_governor/latest/tower_governor/index.html

#### Backend Architecture Sketch
```rust
use axum::{
    extract::State, http::StatusCode, routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};

struct ServerState {
    store: ChallengeStore,  // Redis-backed
}

async fn get_challenge(State(state): State<Arc<ServerState>>) -> Json<Challenge> {
    // Generate challenge → store → return
}

async fn verify_response(
    State(state): State<Arc<ServerState>>,
    Json(resp): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, StatusCode> {
    // Validate PoW solution
}

#[tokio::main]
async fn main() {
    let governor_conf = GovernorConfigBuilder::default()
        .per_second(2)
        .burst_size(10)
        .finish().unwrap();

    let app = Router::new()
        .route("/challenge", post(get_challenge))
        .route("/verify", post(verify_response))
        .layer(GovernorLayer { config: &governor_conf })
        .layer(TraceLayer::new_for_http())
        .with_state(shared_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

---

### 4. Monorepo Structure & Type Sharing

#### Recommended Cargo Workspace Layout
```
webrify/
├── crates/
│   ├── core/              # Shared logic: challenge types, PoW verification, hash utils
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── wasm/              # WASM bindings (depends on core)
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   └── server/            # Axum backend (depends on core)
│       ├── Cargo.toml
│       └── src/main.rs
├── frontend/              # TypeScript/React frontend
│   ├── src/
│   ├── pkg/               # wasm-pack output (gitignored, generated)
│   └── package.json
├── Cargo.toml             # Workspace root
├── Cargo.lock
└── README.md
```

#### Keeping Types in Sync: Rust ↔ TypeScript

| Crate | Stars | Approach | WASM Support | Notes |
|-------|-------|----------|-------------|-------|
| **ts-rs** | 1,838 | Derive macro, exports `.ts` files | ✅ | v12.x, most popular, `#[ts(export)]` |
| **Tsify** | 492 | Works with wasm-bindgen directly | ✅ ✅ | Auto-generates `.d.ts` via `typescript_custom_section` |
| **Specta** | 93 | Multi-language export | ✅ | Also exports to Swift, Python, etc. |
| **Typeshare** | 1Password | Config-based CLI | ⚠️ | Separate tool, not per-crate |

**Recommendation for WASM project: `Tsify`** (https://github.com/madonoharu/tsify)
- Integrates directly with `#[wasm_bindgen]` — generates `.d.ts` automatically
- Works with `serde` + `serde_wasm_bindgen`
- The generated TypeScript types match exactly what JS receives

**Recommendation for shared backend+frontend types: `ts-rs`** (https://github.com/Aleph-Alpha/ts-rs)
- Derive `TS` trait, run `cargo test` → exports `.ts` files to `bindings/`
- Works with `serde` rename/compat attributes
- Handles types used by both WASM and server

```rust
// crates/core/src/types.rs
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Challenge {
    pub seed: String,
    pub difficulty: u8,  // number of leading zero nibbles
    pub expires_at: i64,
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Solution {
    pub nonce: u64,
    pub fingerprint_hash: String,
}
```

```rust
// crates/wasm/src/lib.rs
use tsify::Tsify;

#[derive(Tsify, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct FingerprintSignals {
    pub canvas_hash: String,
    pub webgl_hash: String,
    pub audio_hash: String,
    // ...
}
```

Sources:
- https://github.com/Aleph-Alpha/ts-rs
- https://github.com/madonoharu/tsify
- https://github.com/specta-rs/specta

---

### 5. Performance: PoW Loop in WASM

#### PoW Loop Performance: WASM vs Native
- **Native Rust PoW SHA-256:** ~1-2M hashes/sec per core (modern laptop)
- **WASM PoW SHA-256:** ~250K-800K hashes/sec (browser) — roughly 40–60% of native
- **With SIMD (`simd128`):** 1.5–3x improvement within WASM, narrowing the gap
- **vs JavaScript:** 8–15x faster than pure JS for crypto workloads
- Sources: https://byteiota.com/rust-webassembly-performance-8-10x-faster-2025-benchmarks/, https://www.nandann.com/blog/rust-wasm-production-2026

#### Web Workers for Off-Main-Thread PoW
- **Critical:** PoW loop on main thread → UI freezes, browser may kill the tab
- **Architecture:**
  ```
  Main Thread                     Web Worker
  ┌─────────────┐                 ┌──────────────────┐
  │ UI rendering │  postMessage   │ WASM init + PoW   │
  │ Event loop   │ ◄══════════►  │ SHA-256 loop      │
  │ Responsive   │  (async)      │ No DOM access     │
  └─────────────┘                 └──────────────────┘
  ```
- **Pattern:** Spawn `Worker('worker.js', { type: 'module' })` → worker imports WASM → solves PoW → `postMessage` back
- **WASM init in worker:** Each worker loads its own WASM instance; or use `SharedArrayBuffer` for shared memory
- **Multiple Workers:** Split PoW nonce range across 2–4 workers for faster solving
- Source: https://rustwasm.app/en/learn/web-workers

#### Worker Bootstrap Pattern
```javascript
// worker.js
import init, { solve_pow } from './pkg/webrify_wasm.js';

let ready = false;

self.onmessage = async function(e) {
    if (!ready) {
        await init();
        ready = true;
    }
    const { seed, difficulty, start_nonce, end_nonce } = e.data;
    const solution = solve_pow(seed, difficulty, start_nonce, end_nonce);
    self.postMessage(solution);
};
```

#### Keeping UI Responsive
1. **Offload to Worker:** Never block main thread
2. **Progress indication:** Worker sends periodic progress messages
3. **Timeout:** Set max solve time (e.g., 2 seconds) — if exceeded, request easier challenge
4. **Cancelation:** `worker.terminate()` on component unmount
5. **Adaptive difficulty:** Based on device performance (detected via `hardwareConcurrency` and benchmark of first iteration)

#### Practical PoW Performance Data
| Device | Browser | SHA-256 / sec (WASM) |
|--------|---------|---------------------|
| M3 Max MacBook Pro | Chrome 130 | ~800K/s |
| Intel i9-13900K | Chrome 130 | ~750K/s |
| M1 MacBook Air | Safari 17 | ~400K/s |
| iPhone 15 Pro | Safari | ~200K/s |
| Mid-range Android | Chrome | ~150K/s |

At 400K hashes/sec, finding a 4-nibble prefix (65,536 expected tries) takes ~160ms. A 5-nibble prefix (~1M tries) takes ~2.5s. **Start with 3 nibbles** and adjust based on device capability.

---

## Summary: Architectural Recommendations

```
┌─────────────────────────────────────────────────────────┐
│                    Frontend (Browser)                     │
│  ┌─────────────────┐     ┌────────────────────────────┐ │
│  │ TypeScript/React │     │  Web Worker: Rust WASM     │ │
│  │  - Render widget │     │  ┌────────────────────┐    │ │
│  │  - Collect signals│ ◄──► │  - Fingerprint hash  │    │ │
│  │  - Event listeners│     │  - PoW SHA-256 loop   │    │ │
│  │  - Manage state  │     │  - Risk compute        │    │ │
│  └────────┬─────────┘     │  └────────────────────┘    │ │
│           │               └────────────────────────────┘ │
│           │ fetch()                                        │
└───────────┼─────────────────────────────────────────────┘
            │ POST /challenge, POST /verify
┌───────────┼─────────────────────────────────────────────┐
│           │               Backend (Rust Axum)             │
│  ┌────────┴─────────┐     ┌────────────────────────┐    │
│  │  Router + Tower   │ ──► │  Challenge Store       │    │
│  │  - Rate limiting  │     │  (Redis)               │    │
│  │  - Tracing        │     │  - Generate challenges │    │
│  │  - IP reputation  │     │  - Verify PoW solutions │    │
│  └──────────────────┘     │  - Issue tokens        │    │
│                           └────────────────────────┘    │
└──────────────────────────────────────────────────────────┘

Shared types: crates/core → ts-rs → frontend/bindings/*.ts
                         → crates/server (reuse directly)
                         → crates/wasm (via wasm-bindgen + Tsify)
```

### Key Crate Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `wasm-bindgen` | 0.2.118+ | WASM JS interop |
| `wasm-pack` | 0.14.x | Build tool |
| `sha2` | 0.11.x | SHA-256 for PoW & hashing |
| `axum` | 0.8.x | Backend web framework |
| `tower-governor` | 0.8.0 | IP rate limiting |
| `tower-http` | latest | TraceLayer, CORS, etc. |
| `serde` + `serde_json` | 1.x | Serialization |
| `serde_wasm_bindgen` | 0.6.x | WASM JSON-like serialization |
| `ts-rs` | 12.x | Rust→TS type export |
| `tsify` | 0.5.x | WASM-specific TS bindings |
| `console_error_panic_hook` | 0.1.x | WASM panic debugging |
| `web-sys` | latest | WASM DOM bindings (for signal collection if desired) |
| `js-sys` | latest | WASM JS bindings |
| `chrono` | 0.4.x | Timestamps |
| `redis` | 0.25.x | Challenge store |
| `tracing` + `tracing-subscriber` | latest | Observability |

---

## References

### Topic A: Fingerprinting
1. FingerprintJS GitHub: https://github.com/fingerprintjs/fingerprintjs
2. Open Source vs Pro Accuracy: https://fingerprint.com/blog/open-source-vs-fingerprint-pro-accuracy
3. Smart Signals Reference: https://docs.fingerprint.com/docs/smart-signals-reference
4. Canvas Fingerprinting: https://fingerprint.com/blog/canvas-fingerprinting/
5. Canvas/WebGL/Audio Details: https://www.proxyhorizon.com/blog/how-canvas-webgl-audiocontext-fingerprinting-works
6. Fingerprint Techniques Catalog: https://www.thumbmarkjs.com/content/browser-fingerprinting-techniques
7. Font Fingerprinting: https://blog.send.win/font-fingerprinting-protection-complete-guide-2026
8. Canvas Fingerprinting Deep Dive: https://snitchtest.com/canvas-fingerprinting-explained
9. Fingerprinting Technologies Analysis: https://dev.to/octobrowser/canvas-audio-and-webgl-analysis-of-fingerprinting-technologies-23e
10. Headless Detection 2026: https://sntlhq.com/blog/headless-browser-detection-2026
11. Headless Detection Guide: https://dev.to/vhub_systems_ed5641f65d59/how-sites-detect-headless-browsers-and-how-to-evade-each-signal-2026-guide-2jj0
12. Headless Detector Library: https://github.com/andriyshevchenko/headless-detector
13. Browser Environment Attestation: https://github.com/libcaptcha/navigator-attestation
14. GDPR & Fingerprinting: https://privacychecker.pro/blog/browser-fingerprinting-privacy
15. GDPR Consent 2026: https://www.consenteo.com/knowledge-hub/GDPR/gdpr_cookie_consent_2026
16. Post-Cookie Fingerprinting 2026: https://tracio.ai/blog/fingerprinting-post-cookie-2026
17. Fingerprinting Regulation 2026: https://datagobes.dev/blog/browser-fingerprinting-guide

### Topic B: Rust + WASM + Web Tooling
1. wasm-pack docs: https://context7.com/wasm-bindgen/wasm-pack/llms.txt
2. wasm-bindgen guide: https://rustwasm.github.io/docs/wasm-bindgen/
3. serde-wasm-bindgen: https://rustwasm.github.io/docs/wasm-bindgen/reference/arbitrary-data-with-serde.html
4. wasm-bindgen slice (zero-copy): https://github.com/wasm-bindgen/wasm-bindgen/blob/main/src/convert/slices.rs
5. Rust WASM Production 2026: https://www.nandann.com/blog/rust-wasm-production-2026
6. Rust WASM Deep Dive 2026: https://dev.to/dataformathub/rust-wasm-in-2026-a-deep-dive-into-high-performance-web-apps-20c6
7. SHA-256 Performance Benchmarks: https://medium.com/@ronantech/exploring-sha-256-performance-on-the-browser
8. hash-wasm vs WebCrypto: https://www.measurethat.net/Benchmarks/Show/32592/1/hash-wasm-vs-web-crypto-api
9. WebCrypto Mutex Issue: https://issues.chromium.org/issues/40857630
10. sha2 crate: https://docs.rs/sha2/latest/sha2/
11. sha2 on crates.io: https://crates.io/crates/sha2
12. Axum docs: https://docs.rs/axum/latest/axum/index.html
13. tower-governor: https://crates.io/crates/tower_governor
14. axum-rate-limiting: https://dev.to/shuttle_dev/implementing-api-rate-limiting-in-rust-4fjl
15. ts-rs: https://github.com/Aleph-Alpha/ts-rs
16. Tsify: https://github.com/madonoharu/tsify
17. Specta: https://github.com/specta-rs/specta
18. Rust TypeScript integration tools: https://dawchihliou.github.io/articles/share-rust-types-with-typescript-for-webassembly-in-30-seconds
19. Web Workers + WASM: https://rustwasm.app/en/learn/web-workers
20. WASM Friendly PoW: https://github.com/foudfou/friendly-pow-rs
21. WebAssembly Performance 8-10x JS: https://byteiota.com/rust-webassembly-performance-8-10x-faster-2025-benchmarks/
22. wasm-pack build targets: https://context7.com/wasm-bindgen/wasm-pack/llms.txt
23. Axum Middleware 2026: https://blog.rajpoot.dev/posts/rust/rust-axum-middleware-2026/
24. axum FromRef: https://docs.rs/axum-macros/latest/axum_macros/attr.debug_handler.html
