#!/usr/bin/env bash
set -euo pipefail
script_root="$(cd "$(dirname "$0")" && pwd)"
package_root="$(cd "$script_root/../proofs/SwarmProofs" && pwd)"
cd "$package_root"
mkdir -p .lake/build/lib/lean
for fixture in Reusable ForbiddenAxiom ForbiddenSorry ForbiddenNamespace ForbiddenUnsafe; do
  lake env lean -R ../fixtures -o ".lake/build/lib/lean/$fixture.olean" "../fixtures/$fixture.lean"
  if lake env lean --run "$script_root/proof-declaration-inventory.lean" "$fixture" > ".lake/$fixture.inventory.json" 2> ".lake/$fixture.audit-error.txt"; then
    if [[ "$fixture" != Reusable ]]; then
      echo "ERROR: forbidden fixture accepted: $fixture" >&2
      exit 1
    fi
  elif [[ "$fixture" == Reusable ]]; then
    cat ".lake/$fixture.audit-error.txt" >&2
    exit 1
  else
    case "$fixture" in
      ForbiddenAxiom) expected="new axiom" ;;
      ForbiddenSorry) expected="unapproved axiom: sorryAx" ;;
      ForbiddenNamespace) expected="escaped generated namespace" ;;
      ForbiddenUnsafe) expected="unsafe/partial declaration" ;;
    esac
    grep -F "$expected" ".lake/$fixture.audit-error.txt"
  fi
done
