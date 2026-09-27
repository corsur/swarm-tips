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
lakefile = root / "lakefile.toml"
(root / "lakefile.toml.original").write_bytes(lakefile.read_bytes())
lakefile.write_text(
    lakefile.read_text() + '\n[[lean_lib]]\nname = "PromotionAudit"\n'
)
