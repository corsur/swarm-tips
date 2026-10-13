#!/usr/bin/env bash
# Grow an upgradeable program's programdata account when the new build is
# larger than the space currently allocated, so the subsequent
# `solana program deploy` upgrade cannot fail on account allocation.
#
# Root cause this guards against: 2026-10-12 04:52Z the coordination-game
# mainnet redeploy failed its first tx ("Account allocation failed ... -32002
# simulation failed") because the new .so needed more space than programdata
# had. Extending explicitly, as its own idempotent step, makes that visible
# and retryable.
#
# Usage: ensure-programdata-len.sh <program_id> <path/to/program.so> <rpc_url> [keypair]
set -euo pipefail

PROG="${1:?program id required}"
SO="${2:?.so path required}"
RPC="${3:?rpc url required}"
KEYPAIR="${4:-$HOME/.config/solana/id.json}"

NEW=$(stat -c %s "$SO" 2>/dev/null || stat -f %z "$SO")
CUR=$(solana program show "$PROG" --url "$RPC" --output json | jq -r .dataLen)
echo "program=$PROG new_so=$NEW bytes programdata=$CUR bytes"

if [ "$NEW" -gt "$CUR" ]; then
  ADD=$(( NEW - CUR + NEW / 10 )) # 10% headroom so small growth doesn't re-trigger
  echo "extending programdata by $ADD bytes"
  solana program extend "$PROG" "$ADD" --url "$RPC" --keypair "$KEYPAIR"
else
  echo "programdata large enough; no extend needed"
fi
