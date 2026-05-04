#!/usr/bin/env bash
set -euo pipefail

# check-upgrade-authority.sh
#
# Verifies the upgrade authority of a deployed Solana program.
# - Devnet: logs the authority (informational).
# - Mainnet-beta: compares against EXPECTED_MULTISIG_PUBKEY env var. Fails on mismatch.

usage() {
  cat <<EOF
Usage: $0 --cluster <devnet|mainnet-beta> --program-id <PUBKEY>

Options:
  --cluster       Solana cluster: devnet or mainnet-beta
  --program-id    The program's public key

Environment variables (mainnet-beta requires exactly one of these):
  EXPECTED_MULTISIG_PUBKEY  The expected Squads multisig upgrade authority.
                            Mismatch with actual authority exits 1.

  ALLOW_EOA_AUTHORITY       Acknowledged-tech-debt opt-out. Set this to the
                            expected EOA pubkey (e.g. founder wallet) when no
                            Squads multisig has been set up yet. Mismatch
                            with actual authority exits 1.

If neither variable is set on mainnet-beta the script exits 1. The point
is to make the program's authority surface explicit in the repo's CI
config — silent skipping is what let us drift from policy unnoticed.

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

# Mainnet-beta: require an explicit expected authority (multisig OR
# acknowledged-EOA). Silent skipping was the failure mode that let us
# drift to "every authority is one EOA" without anyone noticing.
EXPECTED_MULTISIG="${EXPECTED_MULTISIG_PUBKEY:-}"
EXPECTED_EOA="${ALLOW_EOA_AUTHORITY:-}"

if [[ -z "$EXPECTED_MULTISIG" && -z "$EXPECTED_EOA" ]]; then
  echo "ERROR: Neither EXPECTED_MULTISIG_PUBKEY nor ALLOW_EOA_AUTHORITY is set."
  echo "Mainnet authority must be declared explicitly in the repo's CI variables."
  echo "  Set EXPECTED_MULTISIG_PUBKEY=<squads-vault-pda>   (production)"
  echo "  or ALLOW_EOA_AUTHORITY=<eoa-pubkey>               (acknowledged tech debt)"
  exit 1
fi

if [[ -n "$EXPECTED_MULTISIG" && -n "$EXPECTED_EOA" ]]; then
  echo "ERROR: Both EXPECTED_MULTISIG_PUBKEY and ALLOW_EOA_AUTHORITY are set."
  echo "Pick one — the multisig path supersedes the EOA opt-out once a vault exists."
  exit 1
fi

if [[ -n "$EXPECTED_MULTISIG" ]]; then
  if [[ "$AUTHORITY" != "$EXPECTED_MULTISIG" ]]; then
    echo "ERROR: Upgrade authority mismatch on mainnet-beta."
    echo "  Expected (Squads multisig): $EXPECTED_MULTISIG"
    echo "  Actual:                     $AUTHORITY"
    exit 1
  fi
  echo "OK: Upgrade authority matches expected Squads multisig ($EXPECTED_MULTISIG)."
  exit 0
fi

# EOA path — explicitly acknowledged tech debt.
if [[ "$AUTHORITY" != "$EXPECTED_EOA" ]]; then
  echo "ERROR: Upgrade authority mismatch on mainnet-beta."
  echo "  Expected (acknowledged-EOA): $EXPECTED_EOA"
  echo "  Actual:                      $AUTHORITY"
  exit 1
fi

echo "OK: Upgrade authority matches acknowledged EOA ($EXPECTED_EOA)."
echo "NOTE: ALLOW_EOA_AUTHORITY is set — this is tech debt. Set"
echo "      EXPECTED_MULTISIG_PUBKEY instead once a Squads vault is configured."
exit 0
