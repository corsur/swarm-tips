#!/usr/bin/env bash
# Regression test for the 2026-05-09 devnet gameplay E2E sweep RCA.
#
# The on-chain coordination-game and shillbot programs must stay close in
# bytecode size between mainnet and devnet. If devnet drifts (e.g., when
# the devnet deploy CI step fails silently — see workflow `deploy-devnet`
# failing on IDL upgrade with 0xbc4 / AccountNotInitialized), gameplay
# E2E tests fail with on-chain `AccountDidNotDeserialize` because
# accounts created under the new IDL don't fit the deployed program's
# struct expectation.
#
# Implementation note: bytecode hash equality across networks isn't a
# valid check because `anchor build` is non-deterministic (timestamp
# embedded, etc.). Size within a small tolerance is the right proxy:
# wildly different sizes indicate one network is stale.
#
# Run locally or in CI: `bash scripts/check-program-parity.sh`.
# Requires: curl + jq (both standard on GitHub Actions runners).

set -euo pipefail

# Per-program BEHIND tolerance (bytes). coordination_game mainnet is built
# with `--features mainnet` (9ffac19, cross-chain leg-A genesis tag) while
# devnet builds without it, so the two binaries legitimately differ by
# ~12KB of codegen — and programdata `space` never shrinks, so it reflects
# each network's historical max. 16KiB absorbs that structural gap while
# still alarming on gross multi-version staleness (the 2026-05-09 case).
# shillbot builds identically on both networks; keep it tight.
PROGRAMS=(
  "coordination_game:2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P:16384"
  "shillbot:2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi:256"
)

drift_count=0

program_data_size() {
  local network=$1
  local program=$2
  local rpc
  if [[ $network == "mainnet-beta" ]]; then
    rpc="https://api.mainnet-beta.solana.com"
  else
    rpc="https://api.devnet.solana.com"
  fi
  # Step 1: fetch the program account in jsonParsed encoding so we can
  # read the programdata pubkey from `data.parsed.info.programData`.
  local pd
  pd=$(curl -s -X POST -H "Content-Type: application/json" \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getAccountInfo\",\"params\":[\"$program\",{\"encoding\":\"jsonParsed\"}]}" \
    "$rpc" | jq -r '.result.value.data.parsed.info.programData // empty')
  if [[ -z $pd ]]; then
    echo ""
    return
  fi
  # Step 2: fetch the programdata account; its `space` field is the
  # bytecode size in bytes (RPC returns it as a parsed integer).
  curl -s -X POST -H "Content-Type: application/json" \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getAccountInfo\",\"params\":[\"$pd\",{\"encoding\":\"base64\"}]}" \
    "$rpc" | jq -r '.result.value.space // empty'
}

for entry in "${PROGRAMS[@]}"; do
  IFS=: read -r name pubkey behind_tolerance <<<"$entry"
  m_size=$(program_data_size "mainnet-beta" "$pubkey")
  d_size=$(program_data_size "devnet" "$pubkey")
  if [[ -z $m_size || -z $d_size ]]; then
    echo "FAIL: $name programdata missing or unreadable on one network"
    drift_count=$((drift_count + 1))
    continue
  fi
  # ASYMMETRIC by policy (2026-06-14): allow devnet to run AHEAD of mainnet.
  # During testnet-first development devnet carries new features (e.g. the
  # permissionless cross-chain lock) while mainnet stays gated — that is the
  # intended state, not drift, so a larger devnet is a WARNING, not a failure.
  # We still FAIL when devnet is meaningfully SMALLER than mainnet: that is the
  # stale/behind direction (a silently-failed deploy left old bytecode) that
  # caused the 2026-05-09 `AccountDidNotDeserialize` outage. Size is an imperfect
  # ahead/behind proxy, but it catches the dangerous case while unblocking
  # intentional testnet-first deploys.
  if (( d_size + behind_tolerance < m_size )); then
    echo "FAIL: $name devnet is BEHIND mainnet (devnet=$d_size < mainnet=$m_size, diff=$((m_size - d_size)), tolerance=$behind_tolerance)"
    echo "  Likely cause: a deploy-devnet CI run failed (often on anchor's"
    echo "  IDL upgrade with 0xbc4 / AccountNotInitialized when the on-chain"
    echo "  IDL is wedged). Bypass the IDL step by using \`solana program"
    echo "  deploy\` directly in CI; do IDL init/upgrade fail-soft afterwards."
    drift_count=$((drift_count + 1))
  elif (( d_size > m_size + 256 )); then
    echo "WARN: $name devnet is AHEAD of mainnet by $((d_size - m_size)) bytes" \
         "(devnet=$d_size, mainnet=$m_size) — expected during testnet-first dev; mainnet gated."
  else
    echo "OK: $name byte sizes within tolerance (mainnet=$m_size, devnet=$d_size)"
  fi
done

if (( drift_count > 0 )); then
  echo
  echo "FAIL: $drift_count program(s) drifted between mainnet and devnet."
  exit 1
fi

echo
echo "PASS: all programs within size tolerance across networks."
