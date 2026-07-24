#!/usr/bin/env bash
# Build the Webrify Turnstile server in release mode and copy the binary to bin/.
#
# Usage:
#   ./scripts/build-release.sh          # full build (wasm + widget + server)
#   ./scripts/build-release.sh --server # server only (skip wasm + widget rebuild)
#
# The binary is placed at bin/webrify, which is tracked by git for CI/CD.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="$ROOT/bin"
BINARY="$BIN_DIR/webrify"

cd "$ROOT"

SKIP_FRONTEND=false
if [ "${1:-}" = "--server" ]; then
    SKIP_FRONTEND=true
    shift
fi

mkdir -p "$BIN_DIR"

if [ "$SKIP_FRONTEND" = false ]; then
    echo "=== Building WASM package ==="
    just build-wasm

    echo "=== Building frontend widget ==="
    (cd packages/turnstile-widget && npm ci && npm run build)
fi

echo "=== Building server (release) ==="
cargo build --release -p turnstile-server

echo "=== Copying binary to $BINARY ==="
cp target/release/webrify "$BINARY"
chmod +x "$BINARY"

echo "=== Done: $(du -h "$BINARY" | cut -f1) at $BINARY ==="

if [ "$SKIP_FRONTEND" = false ]; then
    echo
    echo "Next: docker build -t webrify:latest ."
fi
