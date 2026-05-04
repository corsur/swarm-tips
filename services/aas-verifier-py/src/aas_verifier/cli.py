"""CLI entrypoint: ``aas-verify <attestation.json> [--rpc <url>]``.

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
        prog="aas-verify",
        description="Verify an AAS v1 attestation against on-chain state.",
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
    rpc_fetcher = make_solana_rpc_fetcher(rpc_url)

    verdict = verify_v1(attestation, SHILLBOT_PROTOCOL, rpc_fetcher, rpc_url)
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
        result = resp.json().get("result", {})
        value = result.get("value")
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
    sys.exit(main())
