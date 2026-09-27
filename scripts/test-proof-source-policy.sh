#!/usr/bin/env bash
set -euo pipefail
script_root="$(cd "$(dirname "$0")" && pwd)"
cd "$script_root/../proofs/SwarmProofs"
mkdir -p .lake/source-policy-tests
for fixture in Reusable AllowedComments ForbiddenAxiom ForbiddenSorry ForbiddenNamespace ForbiddenUnsafe ForbiddenEval ForbiddenMacro ForbiddenImport ForbiddenRunTac; do
  output=".lake/source-policy-tests/$fixture.txt"
  if lake env lean --run "$script_root/proof-source-policy.lean" "../fixtures/$fixture.lean" SwarmProofs.Generated.Fixture Mathlib Zeta23 > "$output" 2>&1; then
    case "$fixture" in
      Reusable|AllowedComments) ;;
      *) echo "ERROR: source policy accepted $fixture" >&2; exit 1 ;;
    esac
  else
    case "$fixture" in
      Reusable|AllowedComments) cat "$output" >&2; exit 1 ;;
      *) grep -E 'forbidden|undeclared import|outside generated namespace|namespace escape' "$output" ;;
    esac
  fi
done
