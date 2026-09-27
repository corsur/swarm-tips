#!/usr/bin/env python3
import importlib.util
import json
import pathlib
import unittest

spec = importlib.util.spec_from_file_location("coverage", pathlib.Path(__file__).with_name("check-proof-export-coverage.py"))
coverage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(coverage)


class CoverageTests(unittest.TestCase):
    def check(self, names, records):
        return coverage.check_coverage({"declarations": [{"name": name} for name in names]}, map(json.dumps, records))

    def test_every_helper_required(self):
        records = [{"in": 1, "str": {"pre": 0, "str": "proof"}}, {"thm": {"name": 1}}]
        with self.assertRaisesRegex(ValueError, "omitted"):
            self.check(["proof", "privateHelper"], records)
        self.assertEqual(self.check(["proof"], records)["roots"], ["proof"])

    def test_forbidden_axiom_even_when_not_a_root(self):
        records = [{"in": 1, "str": {"pre": 0, "str": "sorryAx"}}, {"axiom": {"name": 1}}]
        with self.assertRaisesRegex(ValueError, "unapproved"):
            self.check([], records)

    def test_unsafe_rejected(self):
        records = [{"in": 1, "str": {"pre": 0, "str": "helper"}}, {"def": {"name": 1, "safety": "unsafe"}}]
        with self.assertRaisesRegex(ValueError, "unsafe"):
            self.check(["helper"], records)

    def test_inductive_constructors_and_recursors_required(self):
        records = [{"in": i, "str": {"pre": 0, "str": name}} for i, name in enumerate(["T", "T.mk", "T.rec"], 1)]
        records.append({"inductive": {"types": [{"name": 1}], "ctors": [{"name": 2}], "recs": [{"name": 3}]}})
        self.assertEqual(len(self.check(["T", "T.mk", "T.rec"], records)["exported_declarations"]), 3)


if __name__ == "__main__":
    unittest.main()
