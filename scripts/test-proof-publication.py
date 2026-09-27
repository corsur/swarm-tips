#!/usr/bin/env python3
import importlib.util
import json
import pathlib
import subprocess
import unittest
import urllib.error
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("mirror", pathlib.Path(__file__).with_name("mirror-proof-registry.py"))
mirror = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mirror)


class PublicationTests(unittest.TestCase):
    def test_initial_publication_requires_generation_zero(self):
        with patch.object(mirror, "request", side_effect=urllib.error.HTTPError("url", 404, "missing", {}, None)):
            self.assertEqual(mirror.catalog_precondition("bucket", "token", "a" * 40), "0")

    def test_existing_publication_requires_exact_generation(self):
        metadata = {"generation": "123", "metadata": {"source_commit": "a" * 40}}
        with patch.object(mirror, "request", return_value=json.dumps(metadata).encode()), patch.object(mirror.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)) as run:
            self.assertEqual(mirror.catalog_precondition("bucket", "token", "b" * 40), "123")
            run.assert_called_once_with(["git", "merge-base", "--is-ancestor", "a" * 40, "b" * 40], check=False)

    def test_stale_commit_rejected(self):
        metadata = {"generation": "123", "metadata": {"source_commit": "b" * 40}}
        with patch.object(mirror, "request", return_value=json.dumps(metadata).encode()), patch.object(mirror.subprocess, "run", return_value=subprocess.CompletedProcess([], 1)):
            with self.assertRaisesRegex(SystemExit, "stale"):
                mirror.catalog_precondition("bucket", "token", "a" * 40)

    def test_missing_commit_cannot_overwrite_catalog(self):
        with patch.object(mirror, "request", return_value=b'{"generation":"123"}'):
            with self.assertRaisesRegex(SystemExit, "trustworthy"):
                mirror.catalog_precondition("bucket", "token", "a" * 40)

    def test_retry_requires_identical_immutable_bytes(self):
        conflict = urllib.error.HTTPError("url", 412, "exists", {}, None)
        with patch.object(mirror, "request", side_effect=[conflict, b"same", b'{"generation":"5"}']):
            self.assertEqual(mirror.immutable_upload("bucket", "module", b"same", "token"), "5")
        with patch.object(mirror, "request", side_effect=[conflict, b"different"]):
            with self.assertRaisesRegex(SystemExit, "differs"):
                mirror.immutable_upload("bucket", "module", b"same", "token")


if __name__ == "__main__":
    unittest.main()
