# Webrify Turnstile — self-contained Docker image with embedded Redis.
#
# Multi-stage build:
#   0. wasm-builder  → compile turnstile-wasm via wasm-pack
#   1. widget-builder → build turnstile-widget via Vite (npm)
#   2. server-builder→ compile turnstile-server (cargo --release)
#   3. runtime       → Debian slim + redis-server + binary
#
# Build:  docker build -t webrify:latest .
# Run:    docker-compose up

# ── Stage 0: Build the WASM package ──────────────────────────────────────
FROM rust:1-bookworm AS wasm-builder

# wasm-pack binary installer (much faster than `cargo install`).
RUN rustup target add wasm32-unknown-unknown && \
    curl -sSfL https://rustwasm.github.io/wasm-pack/installer/init.sh | sh

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

RUN wasm-pack build crates/turnstile-wasm --target web --out-dir pkg


# ── Stage 1: Build the frontend widget ───────────────────────────────────
FROM node:20-bookworm-slim AS widget-builder

WORKDIR /app

COPY packages/turnstile-widget/package.json packages/turnstile-widget/package-lock.json ./
RUN npm ci

COPY packages/turnstile-widget/ ./

# Inject the WASM package from stage 0.
COPY --from=wasm-builder /app/crates/turnstile-wasm/pkg/turnstile_wasm.js       wasm/
COPY --from=wasm-builder /app/crates/turnstile-wasm/pkg/turnstile_wasm_bg.wasm  wasm/
COPY --from=wasm-builder /app/crates/turnstile-wasm/pkg/turnstile_wasm.d.ts     wasm/

RUN npm run build


# ── Stage 2: Build the server binary ─────────────────────────────────────
FROM rust:1-bookworm AS server-builder

WORKDIR /app

# Layer 1: fetch deps with dummy source (caching).
# All workspace members' Cargo.toml must be present even if we only build one.
COPY Cargo.toml Cargo.lock ./
COPY crates/turnstile-core/Cargo.toml    crates/turnstile-core/
COPY crates/turnstile-wasm/Cargo.toml    crates/turnstile-wasm/
COPY crates/turnstile-server/Cargo.toml  crates/turnstile-server/

RUN mkdir -p crates/turnstile-core/src \
             crates/turnstile-wasm/src \
             crates/turnstile-server/src && \
    echo '' > crates/turnstile-core/src/lib.rs && \
    echo '' > crates/turnstile-wasm/src/lib.rs && \
    echo 'fn main() {}' > crates/turnstile-server/src/main.rs && \
    cargo build --release -p turnstile-server && \
    rm -rf crates /app/target

# Layer 2: real source + widget dist → final binary.
COPY crates/ crates/
COPY --from=widget-builder /app/dist/ packages/turnstile-widget/dist/

RUN cargo build --release -p turnstile-server && \
    cp target/release/webrify /usr/local/bin/webrify


# ── Stage 3: Runtime ─────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends redis-server ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=server-builder /usr/local/bin/webrify /usr/local/bin/webrify

# Defaults (override via docker-compose or -e).
ENV WEBRIFY_BIND_ADDR=0.0.0.0:3000
ENV WEBRIFY_REDIS_URL=redis://127.0.0.1:6379/0

EXPOSE 3000

# Entrypoint: launch Redis, wait until healthy, then exec the server.
RUN printf '#!/bin/sh\n\
set -e\n\
\n\
redis-server --daemonize yes\n\
\n\
until redis-cli ping > /dev/null 2>&1; do\n\
    echo "waiting for redis..."\n\
    sleep 0.5\n\
done\n\
\n\
echo "redis ready, starting Webrify Turnstile"\n\
exec webrify "$@"\n' > /docker-entrypoint.sh && chmod +x /docker-entrypoint.sh

ENTRYPOINT ["/docker-entrypoint.sh"]
