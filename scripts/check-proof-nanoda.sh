#!/usr/bin/env bash
set -euo pipefail

package_root="${1:?usage: check-proof-nanoda.sh <package-root> <module> [report-root] [fixture-namespace] [stability-target]}"
module_name="${2:?usage: check-proof-nanoda.sh <package-root> <module> [report-root] [fixture-namespace] [stability-target]}"
script_root="$(cd "$(dirname "$0")" && pwd)"
package_root="$(cd "$package_root" && pwd)"
report_root="${3:-$package_root/.lake/proof-audit}"
mkdir -p "$report_root"
report_root="$(cd "$report_root" && pwd)"
lean4export_revision="9fb131bb100eb32ccf6836f14e4f8328d13b6792"
nanoda_revision="3a2407216ee84a75f9e1aead6803d0578be06ae7"
work_root="$(mktemp -d)"
trap 'rm -rf "$work_root"' EXIT

git clone --quiet https://github.com/leanprover/lean4export.git "$work_root/lean4export"
git -C "$work_root/lean4export" checkout --quiet "$lean4export_revision"
cp "$package_root/lean-toolchain" "$work_root/lean4export/lean-toolchain"
(cd "$work_root/lean4export" && lake build)

git clone --quiet https://github.com/ammkrn/nanoda_lib.git "$work_root/nanoda"
git -C "$work_root/nanoda" checkout --quiet "$nanoda_revision"
(cd "$work_root/nanoda" && cargo build --release --locked)

exporter_bin="$work_root/lean4export/.lake/build/bin/lean4export"
export_file="$work_root/export.txt"
inventory_file="$report_root/declarations.json"
fixture_namespace="${4:-}"
stability_target="${5:-}"
negative_tampered_mod="${6:-}"
negative_tampered_ns="${7:-}"

inventory_args=("$module_name")
if [[ -n "$fixture_namespace" ]]; then inventory_args+=("$fixture_namespace"); fi
(cd "$package_root" && lake env lean --run "$script_root/proof-declaration-inventory.lean" "${inventory_args[@]}" > "$inventory_file")
# Use every declaration owned by every promoted module, including private and
# generated helpers. The exporter recursively emits their logical dependencies.
cargo xtask export-proof-declarations "$package_root" "$exporter_bin" "$module_name" "$inventory_file" "$export_file"
cargo xtask check-proof-export-coverage "$inventory_file" "$export_file" "$report_root/coverage.json"

if [[ -n "$stability_target" ]]; then
  if [[ -d "$stability_target" ]]; then
    cargo xtask check-promoted-statements-expr-stability "$package_root" "$exporter_bin" "$stability_target"
  elif [[ -n "$fixture_namespace" ]]; then
    cargo xtask check-statement-expr-stability "$package_root" "$exporter_bin" "$module_name" "$stability_target" "$fixture_namespace"
  fi
fi

if [[ -n "$negative_tampered_mod" && -n "$negative_tampered_ns" && -f "$stability_target" ]]; then
  if cargo xtask check-statement-expr-stability "$package_root" "$exporter_bin" "$negative_tampered_mod" "$stability_target" "$negative_tampered_ns" > "$work_root/tampered.out" 2>&1; then
    echo "ERROR: check-statement-expr-stability accepted tampered module $negative_tampered_mod" >&2
    exit 1
  fi
  grep -F "Expr drift detected" "$work_root/tampered.out"
fi

config_file="$work_root/nanoda.json"
printf '%s\n' \
  '{' \
  "  \"export_file_path\": \"$export_file\"," \
  '  "use_stdin": false,' \
  '  "permitted_axioms": ["propext", "Classical.choice", "Quot.sound"],' \
  '  "unpermitted_axiom_hard_error": true,' \
  '  "nat_extension": true,' \
  '  "string_extension": true,' \
  '  "print_success_message": true,' \
  '  "print_axioms": false' \
  '}' > "$config_file"

"$work_root/nanoda/target/release/nanoda_bin" "$config_file"
cargo xtask write-proof-verification-receipt "$report_root" "$lean4export_revision" "$nanoda_revision"
