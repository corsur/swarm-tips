#!/usr/bin/env bash
set -euo pipefail

# check-upgrade-authority.sh
#
# Verifies the upgrade authority of a deployed Solana program.
# - Devnet: logs the authority (informational).
# - Mainnet-beta: compares against EXPECTED_AUTHORITY_PUBKEY env var. Fails on mismatch.

usage() {
  cat <<EOF
Usage: $0 --cluster <devnet|mainnet-beta> --program-id <PUBKEY>

Options:
  --cluster       Solana cluster: devnet or mainnet-beta
  --program-id    The program's public key

Environment variables:
  EXPECTED_AUTHORITY_PUBKEY  The expected upgrade authority (mainnet-beta only).
                             Mismatch with actual authority exits 1. The point
                             is to make the authority surface explicit in CI
                             so we don't drift unnoticed.

Devnet: logs the authority and exits 0 (informational only).
EOF
  exit 1
}

CLUSTER=""
PROGRAM_ID=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --cluster)
      CLUSTER="$2"
      shift 2
      ;;
    --program-id)
      PROGRAM_ID="$2"
      shift 2
      ;;
    --help|-h)
      usage
      ;;
    *)
      echo "ERROR: Unknown argument: $1"
      usage
      ;;
  esac
done

if [[ -z "$CLUSTER" || -z "$PROGRAM_ID" ]]; then
  echo "ERROR: --cluster and --program-id are required."
  usage
fi

if [[ "$CLUSTER" != "devnet" && "$CLUSTER" != "mainnet-beta" ]]; then
  echo "ERROR: --cluster must be 'devnet' or 'mainnet-beta', got '$CLUSTER'."
  exit 1
fi

echo "Checking upgrade authority for program $PROGRAM_ID on $CLUSTER..."

# Fetch program info. Capture both stdout and stderr so we can detect errors.
OUTPUT=""
if ! OUTPUT=$(solana program show "$PROGRAM_ID" --url "$CLUSTER" 2>&1); then
  echo "ERROR: Failed to query program $PROGRAM_ID on $CLUSTER."
  echo "solana program show output:"
  echo "$OUTPUT"
  exit 1
fi

# Parse the Authority field from the output.
AUTHORITY=$(echo "$OUTPUT" | grep -E "^Authority:" | awk '{print $2}')

if [[ -z "$AUTHORITY" ]]; then
  echo "ERROR: Could not parse Authority field from program info."
  echo "Full output:"
  echo "$OUTPUT"
  exit 1
fi

echo "Program $PROGRAM_ID upgrade authority: $AUTHORITY"

if [[ "$CLUSTER" == "devnet" ]]; then
  echo "INFO: Devnet deployment — authority check is informational only."
  exit 0
fi

# Mainnet-beta: require EXPECTED_AUTHORITY_PUBKEY explicitly. Silent skipping
# is what let us drift away from declared policy without anyone noticing.
EXPECTED="${EXPECTED_AUTHORITY_PUBKEY:-}"

if [[ -z "$EXPECTED" ]]; then
  echo "ERROR: EXPECTED_AUTHORITY_PUBKEY is not set."
  echo "Mainnet authority must be declared explicitly in the repo's CI variables."
  echo "  Set EXPECTED_AUTHORITY_PUBKEY=<expected-pubkey>"
  exit 1
fi

if [[ "$AUTHORITY" != "$EXPECTED" ]]; then
  echo "ERROR: Upgrade authority mismatch on mainnet-beta."
  echo "  Expected: $EXPECTED"
  echo "  Actual:   $AUTHORITY"
  exit 1
fi

echo "OK: Upgrade authority matches expected ($EXPECTED)."
exit 0
