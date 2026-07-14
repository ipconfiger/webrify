#!/usr/bin/env bash
# Webrify Turnstile — Docker image build script.
#
# Builds the self-contained image (Rust server + embedded Redis).
# The full pipeline (wasm → widget → server → runtime) runs inside Docker.
#
# Usage:
#   ./scripts/docker-build.sh              # build as webrify:latest
#   ./scripts/docker-build.sh -t mytag      # custom tag
#   ./scripts/docker-build.sh --no-cache    # force full rebuild

set -euo pipefail

TAG="webrify:latest"
CACHE_ARG=""
CONTEXT="$(cd "$(dirname "$0")/.." && pwd)"

while [[ $# -gt 0 ]]; do
    case "$1" in
        -t|--tag)
            TAG="$2"
            shift 2
            ;;
        --no-cache)
            CACHE_ARG="--no-cache"
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [-t TAG] [--no-cache]"
            echo "Build the webrify Docker image (server + embedded Redis)."
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

echo "=== Building $TAG ==="
echo "Context: $CONTEXT"
echo ""

docker build $CACHE_ARG -t "$TAG" "$CONTEXT"

echo ""
echo "=== Build complete: $TAG ==="
echo ""
echo "To run:"
echo "  docker-compose up -d"
echo "  # or manually:"
echo "  docker run -d -p 3000:3000 \\"
echo "    -e WEBRIFY_HMAC_KEY=secret \\"
echo "    -e WEBRIFY_JWT_KEY=secret \\"
echo "    -e WEBRIFY_ALLOWED_ORIGINS=https://example.com \\"
echo "    $TAG"
