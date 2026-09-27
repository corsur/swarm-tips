#!/usr/bin/env bash
set -euo pipefail

package_root="${1:?usage: check-proof-nanoda.sh <package-root> <module>}"
module_name="${2:?usage: check-proof-nanoda.sh <package-root> <module>}"
script_root="$(cd "$(dirname "$0")" && pwd)"
package_root="$(cd "$package_root" && pwd)"
report_root="${3:-$package_root/.lake/proof-audit}"
mkdir -p "$report_root"
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

export_file="$work_root/export.txt"
inventory_file="$report_root/declarations.json"
(cd "$package_root" && lake env lean --run "$script_root/proof-declaration-inventory.lean" "$module_name" > "$inventory_file")
# Use every declaration owned by every promoted module, including private and
# generated helpers. The exporter recursively emits their logical dependencies.
python3 - "$package_root" "$work_root/lean4export/.lake/build/bin/lean4export" "$module_name" "$inventory_file" "$export_file" <<'PY'
import json, subprocess, sys
package, exporter, module, inventory, output = sys.argv[1:]
roots = [item["name"] for item in json.load(open(inventory))["declarations"]]
with open(output, "w") as stream:
    subprocess.run(["lake", "env", exporter, module, "--", *roots], cwd=package, stdout=stream, check=True)
PY
python3 "$script_root/check-proof-export-coverage.py" "$inventory_file" "$export_file" "$report_root/coverage.json"

config_file="$work_root/nanoda.json"
printf '%s\n' \
  '{' \
  "  \"export_file_path\": \"$export_file\"," \
  '  "use_stdin": false,' \
  '  "permitted_axioms": ["propext", "Classical.choice", "Quot.sound"],' \
  '  "unpermitted_axiom_hard_error": true,' \
  '  "nat_extension": true,' \
  '  "string_extension": true,' \
  '  "print_success_message": true' \
  '}' > "$config_file"

"$work_root/nanoda/target/release/nanoda_bin" "$config_file"
python3 - "$report_root" "$lean4export_revision" "$nanoda_revision" <<'PY'
import hashlib, json, pathlib, sys
root = pathlib.Path(sys.argv[1])
coverage = json.loads((root / "coverage.json").read_text())
receipt = {
    "schema": "swarm.lean-independent-verification/v1",
    "independent_kernel": True,
    "lean4export_revision": sys.argv[2],
    "nanoda_revision": sys.argv[3],
    "allowed_axioms": ["propext", "Classical.choice", "Quot.sound"],
    "coverage_sha256": hashlib.sha256((root / "coverage.json").read_bytes()).hexdigest(),
    "inventory_sha256": coverage["inventory_sha256"],
    "export_sha256": coverage["export_sha256"],
    "root_count": len(coverage["roots"]),
    "checked_declaration_count": len(coverage["exported_declarations"]),
}
(root / "verification.json").write_text(json.dumps(receipt, sort_keys=True, indent=2) + "\n")
PY
