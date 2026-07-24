# Webrify Turnstile — self-contained Docker image with embedded Redis.
#
# Pre-requisite: run `./scripts/build-release.sh` on the host first.
# This produces `bin/webrify`, which is then copied into the image.
#
# Build:  ./scripts/build-release.sh && docker build -t webrify:latest .
# Run:    docker-compose up

# ── Runtime ──────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends redis-server ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Pre-built release binary (produced by scripts/build-release.sh on the host).
COPY bin/webrify /usr/local/bin/webrify

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
