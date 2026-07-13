# Webrify Turnstile — build orchestration.
# Full pipeline (wasm-pack -> vite -> cargo) activates in Phase 1.10+.

default: check

# Type-check (default-members only; turnstile-wasm built via wasm-pack).
check:
    cargo check

# Run Rust tests (default-members).
test:
    cargo test

# Lint: clippy must be clean, formatting must match.
lint:
    cargo clippy --all-targets -- -D warnings
    cargo fmt --check

# Auto-format.
fmt:
    cargo fmt

# Build the server binary (and its deps).
build:
    cargo build

# Run the server.
run:
    cargo run

# --- Phase 1.10+ targets (stubs until the frontend is wired) ---

# Build the WASM package (Phase 1.11). Target is explicit per review MINOR.
build-wasm:
    wasm-pack build crates/turnstile-wasm --target web --out-dir pkg

# Build the frontend widget (Phase 1.12). Copies the wasm pkg in first so the
# widget build is self-contained, then runs vite build.
build-widget:
    mkdir -p packages/turnstile-widget/wasm
    cp crates/turnstile-wasm/pkg/turnstile_wasm.js packages/turnstile-widget/wasm/
    cp crates/turnstile-wasm/pkg/turnstile_wasm_bg.wasm packages/turnstile-widget/wasm/
    cp crates/turnstile-wasm/pkg/turnstile_wasm.d.ts packages/turnstile-widget/wasm/
    cd packages/turnstile-widget && npm run build

# Full ordered build (Phase 1.10+): wasm -> widget -> server.
build-all: build-wasm build-widget
    cargo build
