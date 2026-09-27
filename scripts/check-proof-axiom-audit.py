#!/usr/bin/env python3
"""Fail unless every promoted theorem uses only Lean's allowed standard axioms."""

import json
import pathlib
import re
import sys


text = pathlib.Path(sys.argv[1]).read_text()
for forbidden in ("sorryAx", "Lean.ofReduceBool", "Lean.trustCompiler"):
    if forbidden in text:
        raise SystemExit(f"forbidden axiom in audit: {forbidden}")
lines = [line for line in text.splitlines() if "depends on axioms:" in line]
allowed = {"propext", "Classical.choice", "Quot.sound"}
if not lines:
    if len(sys.argv) > 2:
        registry = json.loads(pathlib.Path(sys.argv[2]).read_text())
        if not registry["modules"]:
            print("audited empty initial registry")
            raise SystemExit(0)
    raise SystemExit("axiom audit produced no theorem lines")
for line in lines:
    match = re.search(r"depends on axioms: \[(.*)\]", line)
    if not match:
        raise SystemExit(f"unrecognized axiom audit line: {line}")
    found = {item.strip() for item in match.group(1).split(",") if item.strip()}
    if not found.issubset(allowed):
        raise SystemExit(f"unapproved axioms: {sorted(found - allowed)}")
print(f"audited {len(lines)} exported theorem(s)")
