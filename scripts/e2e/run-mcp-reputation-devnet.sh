#!/usr/bin/env bash
# Hands-off runner for the agent-facing (MCP) reputation e2e: build + boot the
# real mcp-server pointed at devnet, wait for health, drive it through the /mcp
# transport (mcp-reputation-devnet.ts), then tear the server down. Used both
# locally and by .github/workflows/e2e.yml. Exit code is the e2e's.
#
# Env (all optional):
#   PORT             server port (default 8090)
#   SOLANA_RPC_URL   devnet RPC (default https://api.devnet.solana.com)
#   GCP_PROJECT_ID   for the server's Firestore/Secret-Manager client
#                    (default coordination-game-prod). The reputation tools
#                    read devnet directly and need no GCP, but the server's
#                    listings/game state opens a Firestore client at boot, so a
#                    GCP credential (ADC locally / workload-identity in CI) must
#                    be present for the process to start.
set -euo pipefail

PORT="${PORT:-8090}"
SOLANA_RPC_URL="${SOLANA_RPC_URL:-https://api.devnet.solana.com}"
GCP_PROJECT_ID="${GCP_PROJECT_ID:-coordination-game-prod}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

echo "==> building mcp-server (release)"
cargo build --release -p mcp-server

echo "==> booting mcp-server on :$PORT (devnet)"
GCP_PROJECT_ID="$GCP_PROJECT_ID" \
SOLANA_NETWORK=devnet \
SOLANA_RPC_URL="$SOLANA_RPC_URL" \
PORT="$PORT" \
  ./target/release/mcp-server >/tmp/mcp-e2e-server.log 2>&1 &
SERVER_PID=$!
cleanup() {
  kill "$SERVER_PID" 2>/dev/null || true
  wait "$SERVER_PID" 2>/dev/null || true
}
trap cleanup EXIT

echo "==> waiting for /health"
for i in $(seq 1 60); do
  if curl -fsS "http://localhost:$PORT/health" >/dev/null 2>&1; then
    echo "    healthy after ${i}s"
    break
  fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "!!! mcp-server exited during startup; log:" >&2
    cat /tmp/mcp-e2e-server.log >&2
    exit 1
  fi
  sleep 1
done

echo "==> running MCP reputation e2e"
MCP_URL="http://localhost:$PORT" npx tsx scripts/e2e/mcp-reputation-devnet.ts
