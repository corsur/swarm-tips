#!/usr/bin/env python3
"""Publish immutable module bytes, then activate the matching registry last."""

import argparse
import hashlib
import json
import pathlib
import subprocess
import urllib.error
import urllib.parse
import urllib.request
import uuid


def token() -> str:
    return subprocess.check_output(
        ["gcloud", "auth", "print-access-token"], text=True
    ).strip()


def request(url: str, access_token: str, data: bytes | None = None, content_type: str = "application/octet-stream") -> bytes:
    method = "POST" if data is not None else "GET"
    headers = {"Authorization": f"Bearer {access_token}"}
    if data is not None:
        headers["Content-Type"] = content_type
    with urllib.request.urlopen(
        urllib.request.Request(url, data=data, method=method, headers=headers)
    ) as response:
        return response.read()


def immutable_upload(bucket: str, name: str, payload: bytes, access_token: str) -> str:
    encoded_bucket = urllib.parse.quote(bucket, safe="")
    encoded_name = urllib.parse.quote(name, safe="")
    upload = (
        f"https://storage.googleapis.com/upload/storage/v1/b/{encoded_bucket}/o"
        f"?uploadType=media&ifGenerationMatch=0&name={encoded_name}"
    )
    try:
        return json.loads(request(upload, access_token, payload))["generation"]
    except urllib.error.HTTPError as error:
        if error.code != 412:
            raise
    existing = request(
        f"https://storage.googleapis.com/storage/v1/b/{encoded_bucket}/o/"
        f"{encoded_name}?alt=media",
        access_token,
    )
    if existing != payload:
        raise SystemExit(f"immutable object differs from local bytes: gs://{bucket}/{name}")
    return json.loads(request(f"https://storage.googleapis.com/storage/v1/b/{encoded_bucket}/o/{encoded_name}", access_token))["generation"]


def catalog_precondition(bucket: str, access_token: str, source_commit: str) -> str:
    """Reject stale/divergent commits and return the exact CAS generation."""
    url = f"https://storage.googleapis.com/storage/v1/b/{urllib.parse.quote(bucket, safe='')}/o/registry.json"
    try:
        metadata = json.loads(request(url, access_token))
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return "0"
        raise
    previous = metadata.get("metadata", {}).get("source_commit", "")
    if len(previous) != 40 or any(c not in "0123456789abcdef" for c in previous):
        raise SystemExit("existing catalog has no trustworthy source commit; refusing overwrite")
    if subprocess.run(["git", "merge-base", "--is-ancestor", previous, source_commit], check=False).returncode:
        raise SystemExit("refusing stale or divergent catalog publication")
    return metadata["generation"]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project", required=True)
    parser.add_argument("--package", required=True, type=pathlib.Path)
    args = parser.parse_args()
    registry_path = args.package / "registry.json"
    registry_bytes = registry_path.read_bytes()
    registry = json.loads(registry_bytes)
    bucket = f"{args.project}-swarm-proof-registry-public"
    access_token = token()
    source_commit = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    generation = catalog_precondition(bucket, access_token, source_commit)
    objects = []
    for module in registry["modules"]:
        module_id = module["module_id"].removeprefix("sha256:")
        source_path = args.package / (module["module_name"].replace(".", "/") + ".lean")
        source = source_path.read_bytes()
        if hashlib.sha256(source).hexdigest() != module["source_sha256"] or len(source) != module["source_bytes"]:
            raise SystemExit(f"source hash mismatch: {source_path}")
        name = f"modules/{module_id}.lean"
        object_generation = immutable_upload(bucket, name, source, access_token)
        objects.append({"name": name, "generation": object_generation, "module_id": module["module_id"], "source_sha256": module["source_sha256"]})
    registry_hash = hashlib.sha256(registry_bytes).hexdigest()
    immutable_upload(bucket, f"catalogs/{registry_hash}.json", registry_bytes, access_token)
    receipt = {"schema": "swarm.lean-publication/v1", "source_commit": source_commit, "registry_sha256": registry_hash, "objects": objects}
    immutable_upload(bucket, f"publications/{source_commit}.json", json.dumps(receipt, sort_keys=True, separators=(",", ":")).encode(), access_token)
    encoded_bucket = urllib.parse.quote(bucket, safe="")
    encoded_name = urllib.parse.quote("registry.json", safe="")
    boundary = uuid.uuid4().hex
    metadata = json.dumps({"name": "registry.json", "metadata": {"source_commit": source_commit, "registry_sha256": registry_hash}}).encode()
    multipart = (f"--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n".encode() + metadata
        + f"\r\n--{boundary}\r\nContent-Type: application/json\r\n\r\n".encode() + registry_bytes + f"\r\n--{boundary}--\r\n".encode())
    request(
        f"https://storage.googleapis.com/upload/storage/v1/b/{encoded_bucket}/o"
        f"?uploadType=multipart&ifGenerationMatch={generation}",
        access_token,
        multipart,
        f"multipart/related; boundary={boundary}",
    )
    mirrored = request(
        f"https://storage.googleapis.com/storage/v1/b/{encoded_bucket}/o/"
        f"{encoded_name}?alt=media",
        access_token,
    )
    if mirrored != registry_bytes:
        raise SystemExit("registry mirror did not return the committed bytes")
    print(f"mirrored registry sha256={hashlib.sha256(registry_bytes).hexdigest()}")


if __name__ == "__main__":
    main()
