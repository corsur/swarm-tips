#!/usr/bin/env python3
"""Create an ephemeral umbrella and axiom audit for generated proof modules."""

import json
import pathlib


root = pathlib.Path(__file__).resolve().parents[1] / "proofs" / "SwarmProofs"
registry = json.loads((root / "registry.json").read_text())
modules = [module for module in registry["modules"] if module["status"] == "active"]
imports = "\n".join(f'import {module["module_name"]}' for module in modules)
axioms = "\n".join(f'#print axioms {module["theorem_name"]}' for module in modules)
(root / "PromotionAudit.lean").write_text(
    "/- SPDX-License-Identifier: Apache-2.0 -/\n" + imports + "\n" + axioms + "\n"
)
root_module = root / "SwarmProofs.lean"
(root / "SwarmProofs.lean.original").write_bytes(root_module.read_bytes())
root_module.write_text(root_module.read_text() + "\nimport PromotionAudit\n")
