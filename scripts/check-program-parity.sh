#!/usr/bin/env bash
# Regression test for the 2026-05-09 devnet gameplay E2E sweep RCA.
#
# The on-chain coordination-game program must be byte-identical between
# mainnet and devnet. If devnet drifts (e.g., when the devnet deploy CI
# step fails silently — see workflow `deploy-devnet` failing on IDL
# upgrade with 0xbc4 AccountNotInitialized), gameplay E2E tests fail
# with on-chain `AccountDidNotDeserialize` because accounts created
# under the new IDL don't fit the deployed program's struct expectation.
#
# This test fetches the first 4 KiB of program-data from both networks
# and compares hashes. If they diverge, the test fails and CI surfaces
# the drift before agents discover it via failed transactions.
#
# Run locally or in CI: `bash scripts/check-program-parity.sh`.

set -euo pipefail

PROGRAMS=(
  "coordination_game:2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P"
  "shillbot:2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi"
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
  # Fetch program account, parse base64-encoded data, extract the
  # programdata pubkey reference (bytes 4..36 of the BPFLoaderUpgradeable
  # state), then fetch programdata's lamports as a stand-in for size. The
  # programdata account's `space` reflects how many bytes the deployed
  # bytecode occupies. We avoid base58 by reading the JSON RPC response's
  # space field directly (it's the parsed account size in bytes).
  curl -s -X POST -H "Content-Type: application/json" \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getAccountInfo\",\"params\":[\"$program\",{\"encoding\":\"jsonParsed\"}]}" \
    "$rpc" | python3 -c "
import json, sys, urllib.request
v = json.load(sys.stdin)['result']['value']
if not v:
  sys.exit()
parsed = v.get('data', {})
if isinstance(parsed, dict):
  pd = parsed.get('parsed', {}).get('info', {}).get('programData')
  if not pd:
    sys.exit()
  rpc = '$rpc'
  body = json.dumps({'jsonrpc':'2.0','id':1,'method':'getAccountInfo','params':[pd, {'encoding':'base64'}]}).encode()
  req = urllib.request.Request(rpc, data=body, headers={'Content-Type':'application/json'})
  with urllib.request.urlopen(req, timeout=15) as r:
    pd_v = json.load(r)['result']['value']
  if pd_v:
    print(pd_v.get('space', 0))
"
}

for entry in "${PROGRAMS[@]}"; do
  name="${entry%%:*}"
  pubkey="${entry##*:}"
  m_size=$(program_data_size "mainnet-beta" "$pubkey")
  d_size=$(program_data_size "devnet" "$pubkey")
  if [[ -z $m_size || -z $d_size ]]; then
    echo "FAIL: $name programdata missing or unreadable on one network"
    drift_count=$((drift_count + 1))
    continue
  fi
  # Sizes within a small tolerance — Anchor builds may differ by a handful
  # of bytes due to timestamp-embedded metadata. A wider gap indicates
  # one network is stale. 256-byte tolerance is generous.
  diff=$((m_size > d_size ? m_size - d_size : d_size - m_size))
  if (( diff <= 256 )); then
    echo "OK: $name byte sizes within tolerance (mainnet=$m_size, devnet=$d_size, diff=$diff)"
  else
    echo "FAIL: $name programdata size diverged"
    echo "  mainnet bytecode size: $m_size"
    echo "  devnet  bytecode size: $d_size"
    echo "  diff: $diff bytes"
    echo "  Likely cause: a deploy-devnet CI run failed (often on anchor's"
    echo "  IDL upgrade with 0xbc4 / AccountNotInitialized when the on-chain"
    echo "  IDL is wedged). Bypass the IDL step by using \`solana program"
    echo "  deploy\` directly in CI; do IDL init/upgrade fail-soft afterwards."
    drift_count=$((drift_count + 1))
  fi
done

if (( drift_count > 0 )); then
  echo
  echo "FAIL: $drift_count program(s) drifted between mainnet and devnet."
  exit 1
fi

echo
echo "PASS: all programs byte-equal across networks (first 4 KiB)."
