"""Flow tests for the CLI's Solana RPC fetcher (mocked I/O, no network).

The fetcher is the CLI's one I/O orchestrator (HTTP POST -> status check ->
JSON decode -> base64 decode); these cover its happy path and each error
path, including the JSON-RPC-error-as-HTTP-200 shape that must abort the
verification rather than read as "account doesn't exist".
"""

from __future__ import annotations

import base64

import pytest

from vow_verifier.cli import make_solana_rpc_fetcher


class _FakeResponse:
    def __init__(self, payload, status=200):
        self._payload = payload
        self.status_code = status

    def raise_for_status(self):
        if self.status_code >= 400:
            raise RuntimeError(f"HTTP {self.status_code}")

    def json(self):
        return self._payload


def _post_returning(payload, status=200):
    def post(url, json=None, timeout=None):  # noqa: A002 — requests' kwarg name
        return _FakeResponse(payload, status)

    return post


def test_fetch_happy_path_decodes_owner_and_data(monkeypatch):
    data = b"\x01\x02\x03hello"
    payload = {
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "value": {
                "owner": "own3rB58",
                "data": [base64.b64encode(data).decode(), "base64"],
            }
        },
    }
    monkeypatch.setattr("vow_verifier.cli.requests.post", _post_returning(payload))
    fetch = make_solana_rpc_fetcher("http://rpc.test")
    assert fetch("acct") == ("own3rB58", data)


def test_fetch_missing_account_returns_none(monkeypatch):
    payload = {"jsonrpc": "2.0", "id": 1, "result": {"value": None}}
    monkeypatch.setattr("vow_verifier.cli.requests.post", _post_returning(payload))
    fetch = make_solana_rpc_fetcher("http://rpc.test")
    assert fetch("acct") is None


def test_fetch_jsonrpc_error_raises_not_none(monkeypatch):
    # HTTP 200 with an `error` object and no `result` — must raise, never
    # masquerade as a missing account.
    payload = {
        "jsonrpc": "2.0",
        "id": 1,
        "error": {"code": -32005, "message": "node is behind"},
    }
    monkeypatch.setattr("vow_verifier.cli.requests.post", _post_returning(payload))
    fetch = make_solana_rpc_fetcher("http://rpc.test")
    with pytest.raises(RuntimeError, match="RPC error"):
        fetch("acct")


def test_fetch_http_error_raises(monkeypatch):
    monkeypatch.setattr(
        "vow_verifier.cli.requests.post", _post_returning({}, status=503)
    )
    fetch = make_solana_rpc_fetcher("http://rpc.test")
    with pytest.raises(RuntimeError, match="HTTP 503"):
        fetch("acct")
