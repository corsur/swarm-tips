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

hash_program_data() {
  local network=$1
  local program=$2
  local rpc
  if [[ $network == "mainnet-beta" ]]; then
    rpc="https://api.mainnet-beta.solana.com"
  else
    rpc="https://api.devnet.solana.com"
  fi
  local pd
  pd=$(curl -s -X POST -H "Content-Type: application/json" \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getAccountInfo\",\"params\":[\"$program\",{\"encoding\":\"base64\"}]}" \
    "$rpc" | python3 -c "
import json, sys, base64, base58
v = json.load(sys.stdin)['result']['value']
if not v:
  print(''); sys.exit()
print(base58.b58encode(base64.b64decode(v['data'][0])[4:36]).decode())
")
  if [[ -z $pd ]]; then
    echo "FAIL: program $program not found on $network"
    exit 1
  fi
  curl -s -X POST -H "Content-Type: application/json" \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getAccountInfo\",\"params\":[\"$pd\",{\"encoding\":\"base64\",\"dataSlice\":{\"offset\":0,\"length\":4096}}]}" \
    "$rpc" | python3 -c "
import json, sys, base64, hashlib
v = json.load(sys.stdin)['result']['value']
if not v:
  print(''); sys.exit()
print(hashlib.sha256(base64.b64decode(v['data'][0])).hexdigest())
"
}

for entry in "${PROGRAMS[@]}"; do
  name="${entry%%:*}"
  pubkey="${entry##*:}"
  m_hash=$(hash_program_data "mainnet-beta" "$pubkey")
  d_hash=$(hash_program_data "devnet" "$pubkey")
  if [[ -z $m_hash || -z $d_hash ]]; then
    echo "FAIL: $name programdata missing on one network"
    drift_count=$((drift_count + 1))
    continue
  fi
  if [[ $m_hash == "$d_hash" ]]; then
    echo "OK: $name first-4KiB hash matches across networks ($m_hash)"
  else
    echo "FAIL: $name programdata diverged"
    echo "  mainnet first 4KiB sha256: $m_hash"
    echo "  devnet  first 4KiB sha256: $d_hash"
    echo "  Likely cause: a deploy-devnet CI run failed on the IDL upgrade"
    echo "  step (anchor deploy returns 0xbc4 / AccountNotInitialized when"
    echo "  the IDL account has drifted from the upgrader's expectation)."
    echo "  Fix: re-initialize the on-chain IDL with anchor idl init, then"
    echo "  re-run the deploy-devnet job. Until they match, accounts"
    echo "  created under the new IDL fail to deserialize on-chain."
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
