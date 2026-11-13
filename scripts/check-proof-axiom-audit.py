#!/usr/bin/env python3
"""Fail unless every promoted theorem uses only Lean's allowed standard axioms."""

import json
import pathlib
import re
import sys


DEPENDS_RE = re.compile(r"^'([^']+)' depends on axioms: \[(.*)\]\s*$")
NO_AXIOMS_RE = re.compile(r"^'([^']+)' does not depend on any axioms\s*$")
ALLOWED = {"propext", "Classical.choice", "Quot.sound"}
FORBIDDEN = ("sorryAx", "Lean.ofReduceBool", "Lean.trustCompiler")

text = pathlib.Path(sys.argv[1]).read_text()
for forbidden in FORBIDDEN:
    if forbidden in text:
        raise SystemExit(f"forbidden axiom in audit: {forbidden}")

audited: dict[str, set[str]] = {}
for raw_line in text.splitlines():
    line = raw_line.strip()
    if not line:
        continue
    if line.startswith("error:") or ": error:" in line:
        raise SystemExit(f"lean error during axiom audit: {line}")
    no_ax = NO_AXIOMS_RE.match(line)
    if no_ax:
        audited[no_ax.group(1)] = set()
        continue
    dep = DEPENDS_RE.match(line)
    if dep:
        found = {item.strip() for item in dep.group(2).split(",") if item.strip()}
        if not found:
            raise SystemExit(f"malformed empty axiom list in audit line: {line}")
        if not found.issubset(ALLOWED):
            raise SystemExit(f"unapproved axioms: {sorted(found - ALLOWED)}")
        audited[dep.group(1)] = found
        continue
    if "depends on axioms" in line or "does not depend on any axioms" in line:
        raise SystemExit(f"unrecognized axiom audit line: {line}")

if len(sys.argv) > 2:
    registry = json.loads(pathlib.Path(sys.argv[2]).read_text())
    active_modules = [
        m for m in registry.get("modules", []) if m.get("status", "active") == "active"
    ]
    if not audited:
        if not active_modules:
            print("audited empty initial registry")
            raise SystemExit(0)
        raise SystemExit("axiom audit produced no theorem lines")
    missing = [
        m["theorem_name"] for m in active_modules if m["theorem_name"] not in audited
    ]
    if missing:
        raise SystemExit(f"missing axiom audit output for theorem(s): {missing}")
elif not audited:
    raise SystemExit("axiom audit produced no theorem lines")

print(f"audited {len(audited)} exported theorem(s)")
