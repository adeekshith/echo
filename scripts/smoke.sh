#!/usr/bin/env bash
#
# Container smoke tests for the ipecho production image.
#
# Covers checks that only make sense against a built image (non-root user,
# HEALTHCHECK presence, SIGTERM graceful shutdown) — things that can't be
# expressed as a cargo test because they exercise the Dockerfile itself or
# require delivering real signals to a running process.
#
# Usage:
#   ./scripts/smoke.sh [image-tag]   # default: ipecho-run
#
# Exits non-zero on the first failure. Intended for CI; safe to run locally.

set -euo pipefail

IMG=${1:-ipecho-run}
HOST_PORT=${HOST_PORT:-18083}
CONTAINER_NAME="ipecho-smoke-$$"

log()  { printf '\033[1;34m[smoke]\033[0m %s\n' "$*"; }
pass() { printf '\033[1;32m[ ok  ]\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31m[fail ]\033[0m %s\n' "$*" >&2; exit 1; }

cleanup() {
    docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# 1. Image must exist
# ---------------------------------------------------------------------------
log "checking image $IMG exists"
docker image inspect "$IMG" >/dev/null 2>&1 \
    || fail "image '$IMG' not found — run 'docker build -t $IMG .' first"
pass "image present"

# ---------------------------------------------------------------------------
# 2. Container runs as uid 10001 (not root)
# ---------------------------------------------------------------------------
log "checking container runs as non-root"
uid=$(docker run --rm --entrypoint id "$IMG" -u)
if [[ "$uid" != "10001" ]]; then
    fail "expected uid 10001, got '$uid'"
fi
pass "container runs as uid=10001"

# ---------------------------------------------------------------------------
# 3. HEALTHCHECK declared and references /health
# ---------------------------------------------------------------------------
log "checking HEALTHCHECK is declared"
hc=$(docker inspect --format '{{if .Config.Healthcheck}}{{.Config.Healthcheck.Test}}{{else}}<none>{{end}}' "$IMG")
if [[ "$hc" == "<none>" ]]; then
    fail "image has no HEALTHCHECK instruction"
fi
if ! grep -q '/health' <<<"$hc"; then
    fail "HEALTHCHECK does not reference /health: $hc"
fi
pass "HEALTHCHECK references /health"

# ---------------------------------------------------------------------------
# 4. SIGTERM drains cleanly: container starts, serves /health, then on
#    TERM exits 0 within 10s and logs 'SIGTERM received'
# ---------------------------------------------------------------------------
log "starting container to test graceful shutdown"
docker run -d --name "$CONTAINER_NAME" -p "${HOST_PORT}:8083" "$IMG" >/dev/null

# Wait up to 10s for /health to respond
log "waiting for /health to come up"
for _ in $(seq 1 50); do
    if curl -fsS "http://localhost:${HOST_PORT}/health" >/dev/null 2>&1; then
        break
    fi
    sleep 0.2
done
curl -fsS "http://localhost:${HOST_PORT}/health" >/dev/null \
    || fail "/health never became reachable within 10s"
pass "/health reachable"

log "sending SIGTERM"
docker kill --signal=TERM "$CONTAINER_NAME" >/dev/null

# Wait up to 10s for the container to exit
for _ in $(seq 1 50); do
    running=$(docker inspect -f '{{.State.Running}}' "$CONTAINER_NAME")
    [[ "$running" == "false" ]] && break
    sleep 0.2
done
running=$(docker inspect -f '{{.State.Running}}' "$CONTAINER_NAME")
if [[ "$running" != "false" ]]; then
    fail "container did not exit within 10s of SIGTERM"
fi

exit_code=$(docker inspect -f '{{.State.ExitCode}}' "$CONTAINER_NAME")
if [[ "$exit_code" != "0" ]]; then
    fail "container exited non-zero after SIGTERM: $exit_code"
fi
pass "container exited 0 after SIGTERM"

if ! docker logs "$CONTAINER_NAME" 2>&1 | grep -q "SIGTERM received"; then
    fail "expected log line 'SIGTERM received' not found"
fi
pass "graceful-shutdown log line present"

log "all smoke tests passed"
