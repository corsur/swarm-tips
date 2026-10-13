#!/usr/bin/env bash
# Fail unless <id> (Solana program id or EVM proxy variable name) is listed in
# .github/mainnet-auto-deploy.yml — the allowlist of contracts CI may push to
# mainnet. Usage: check-mainnet-allowlist.sh <program_id_or_proxy_var>
set -euo pipefail
ID="${1:?id required}"
LIST="$(dirname "$0")/../.github/mainnet-auto-deploy.yml"
if grep -Eq "^[[:space:]]+[a-z0-9_]+:[[:space:]]+${ID}([[:space:]]|$)" "$LIST"; then
  echo "allowlisted for mainnet auto-deploy: $ID"
else
  echo "::error::$ID is not in .github/mainnet-auto-deploy.yml — refusing mainnet deploy" >&2
  exit 1
fi
