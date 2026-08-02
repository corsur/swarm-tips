"""CLI entrypoint: ``vow-verify <attestation.json> [--rpc <url>]``.

MVP — reads an attestation from a file path or stdin, runs the
full 7-step verifier against the named network's default RPC (or
``--rpc`` override), prints the verdict as JSON.

Exit codes: 0 = valid, 1 = invalid (verdict still printed), 2 =
usage error / unreadable file.
"""

from __future__ import annotations

import argparse
import base64
import json
import sys
from typing import Optional, Tuple

import requests

from .decoders.shillbot import SHILLBOT_PROTOCOL
from .verify import verify_v1


_DEFAULT_RPC = {
    "mainnet": "https://api.mainnet-beta.solana.com",
    "devnet": "https://api.devnet.solana.com",
}


def main() -> int:
    parser = argparse.ArgumentParser(
        prog="vow-verify",
        description="Verify a VOW v1 attestation against on-chain state.",
    )
    parser.add_argument(
        "path",
        help='Path to the attestation JSON, or "-" for stdin.',
    )
    parser.add_argument(
        "--rpc",
        help="Override the default RPC endpoint for the attestation's network.",
    )
    args = parser.parse_args()

    try:
        if args.path == "-":
            raw = sys.stdin.read()
        else:
            with open(args.path, "r", encoding="utf-8") as f:
                raw = f.read()
    except OSError as e:
        sys.stderr.write(f"failed to read {args.path}: {e}\n")
        return 2

    try:
        attestation = json.loads(raw)
    except json.JSONDecodeError as e:
        sys.stderr.write(f"invalid JSON in {args.path}: {e}\n")
        return 2

    network = (
        attestation.get("network") if isinstance(attestation, dict) else None
    )
    rpc_url = args.rpc or _DEFAULT_RPC.get(network or "", _DEFAULT_RPC["mainnet"])

    # Infra failures (network down, RPC error, malformed response) are neither
    # "valid" nor "invalid" — exit 2 with a message, mirroring the TS CLI's
    # top-level handler, instead of a raw traceback that collides with the
    # exit-1 "invalid" contract.
    try:
        rpc_fetcher = make_solana_rpc_fetcher(rpc_url)
        verdict = verify_v1(attestation, SHILLBOT_PROTOCOL, rpc_fetcher, rpc_url)
    except Exception as e:  # noqa: BLE001 — boundary: report, don't traceback
        sys.stderr.write(f"verification aborted: {e}\n")
        return 2
    sys.stdout.write(json.dumps(verdict, indent=2, default=str) + "\n")
    return 0 if verdict.get("valid") else 1


def make_solana_rpc_fetcher(rpc_url: str):
    """Returns a callable suitable for ``verify_v1``'s RPC parameter.

    Calls Solana's JSON-RPC ``getAccountInfo`` with base64 encoding,
    decodes the response, and returns ``(owner_b58, data_bytes)`` or
    ``None`` when the account doesn't exist.
    """

    def fetch(account_b58: str) -> Optional[Tuple[str, bytes]]:
        body = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getAccountInfo",
            "params": [
                account_b58,
                {"encoding": "base64", "commitment": "confirmed"},
            ],
        }
        resp = requests.post(rpc_url, json=body, timeout=15)
        resp.raise_for_status()
        payload = resp.json()
        # A JSON-RPC error arrives as HTTP 200 with an `error` object and no
        # `result`. Treating that as "account doesn't exist" would silently
        # flip a verification verdict — raise instead (the CLI reports it as
        # an aborted verification, exit 2).
        if "error" in payload or "result" not in payload:
            raise RuntimeError(f"RPC error from {rpc_url}: {payload.get('error')}")
        value = payload["result"].get("value")
        if value is None:
            return None
        owner = value.get("owner")
        data_field = value.get("data")
        if owner is None or not isinstance(data_field, list) or len(data_field) < 1:
            return None
        # data is [base64, "base64"] in the standard response shape.
        data = base64.b64decode(data_field[0])
        return (owner, data)

    return fetch


if __name__ == "__main__":
    # Match the TS sibling's top-level handler. Without this, an exception
    # escaping main() (an RPC transport failure, an unreadable file) printed a
    # Python traceback and exited 1, which is the same code a legitimate
    # "attestation did not verify" uses — a caller could not tell a failed
    # verification from a crashed verifier.
    try:
        sys.exit(main())
    except Exception as exc:  # noqa: BLE001 - top-level guard, re-reported below
        print(f"unexpected error: {exc}", file=sys.stderr)
        sys.exit(2)
