#!/usr/bin/env python3
"""Parse each catalog source before any submitted module is compiled/imported."""
import hashlib
import json
import pathlib
import subprocess

repository = pathlib.Path(__file__).resolve().parents[1]
package = repository / "proofs" / "SwarmProofs"
registry = json.loads((package / "registry.json").read_text())
modules = {item["module_id"]: item for item in registry["modules"]}
for module in registry["modules"]:
    # Historical modules remain immutable, including when yanked.
    source = package / (module["module_name"].replace(".", "/") + ".lean")
    payload = source.read_bytes()
    if hashlib.sha256(payload).hexdigest() != module["source_sha256"] or len(payload) != module["source_bytes"]:
        raise SystemExit(f"source integrity mismatch: {module['module_id']}")
    closure = set()

    def visit(module_id):
        if module_id in closure:
            return
        closure.add(module_id)
        for dependency in modules[module_id]["direct_dependencies"]:
            visit(dependency)

    for dependency in module["direct_dependencies"]:
        visit(dependency)
    allowed = ["Mathlib", "Zeta23", *sorted(modules[key]["module_name"] for key in closure)]
    namespace = module["theorem_name"].rsplit(".", 1)[0]
    subprocess.run(["lake", "env", "lean", "--run", str(repository / "scripts" / "proof-source-policy.lean"), str(source), namespace, *allowed], cwd=package, check=True)
