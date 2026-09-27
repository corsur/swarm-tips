#!/usr/bin/env python3
"""Require independent-export coverage of every compiled promoted declaration.

Nanoda subsequently checks the complete dependency graph. This extra gate detects
exporter omissions (including silently skipped unsafe/partial declarations).
It is not a substitute for independent kernel acceptance.
"""
import hashlib
import json
import pathlib
import sys

ALLOWED_AXIOMS = {"propext", "Classical.choice", "Quot.sound"}


def check_coverage(inventory, lines):
    roots = [item["name"] for item in inventory["declarations"]]
    if len(roots) != len(set(roots)):
        raise ValueError("duplicate inventory declaration")
    names = {0: ""}
    declarations = set()
    axioms = set()
    for line in lines:
        record = json.loads(line)
        if "in" in record:
            kind = "str" if "str" in record else "num"
            value = record[kind]
            parent = names[value["pre"]]
            part = str(value["str"] if kind == "str" else value["i"])
            names[record["in"]] = f"{parent}.{part}" if parent else part
        entries = []
        for kind in ("axiom", "def", "opaque", "thm", "quot"):
            if kind in record:
                entries.append(record[kind])
                if kind == "axiom":
                    axioms.add(names[record[kind]["name"]])
        if "inductive" in record:
            for kind in ("types", "ctors", "recs"):
                entries.extend(record["inductive"][kind])
        for entry in entries:
            if entry.get("isUnsafe", False) or entry.get("safety", "safe") != "safe":
                raise ValueError("unsafe/partial exported declaration")
            declarations.add(names[entry["name"]])
    missing = set(roots) - declarations
    if missing:
        raise ValueError(f"export omitted declarations: {sorted(missing)}")
    if axioms - ALLOWED_AXIOMS:
        raise ValueError(f"unapproved axioms: {sorted(axioms - ALLOWED_AXIOMS)}")
    return {"schema": "swarm.lean-export-coverage/v1", "roots": sorted(roots),
            "exported_declarations": sorted(declarations), "axioms": sorted(axioms)}


if __name__ == "__main__":
    inventory_path, export_path, report_path = map(pathlib.Path, sys.argv[1:])
    with export_path.open() as stream:
        report = check_coverage(json.loads(inventory_path.read_text()), stream)
    report["inventory_sha256"] = hashlib.sha256(inventory_path.read_bytes()).hexdigest()
    report["export_sha256"] = hashlib.file_digest(export_path.open("rb"), "sha256").hexdigest()
    report_path.write_text(json.dumps(report, sort_keys=True, indent=2) + "\n")
