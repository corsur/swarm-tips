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


def test_p2_26_fetcher_exposes_get_genesis_hash_and_cli_pins_mainnet_by_default(
    monkeypatch, tmp_path, capsys
):
    import json
    from vow_verifier.cli import main

    mainnet_hash = "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d"
    monkeypatch.setattr(
        "vow_verifier.cli.requests.post",
        _post_returning({"jsonrpc": "2.0", "id": 1, "result": mainnet_hash}),
    )
    fetch = make_solana_rpc_fetcher("http://rpc.test")
    assert fetch.get_genesis_hash() == mainnet_hash

    # CLI without --network MUST refuse a devnet attestation with network_mismatch
    # and MUST NOT switch RPC to devnet!
    seen_urls = []

    def fail_if_called(url, json=None, timeout=None):  # noqa: A002
        seen_urls.append(url)
        raise AssertionError(f"should not call RPC on network mismatch: {url}")

    monkeypatch.setattr("vow_verifier.cli.requests.post", fail_if_called)

    att_file = tmp_path / "devnet.json"
    att_file.write_text(
        json.dumps(
            {
                "version": "vow/v1",
                "network": "devnet",
                "program_id": "2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi",
                "account": "11111111111111111111111111111112",
                "account_kind": "Task",
                "task_id": "42",
                "client": "11111111111111111111111111111113",
                "agent": "11111111111111111111111111111114",
                "state": "verified",
                "platform": 0,
                "composite_score": "850000",
                "score_max": "1000000",
                "verified_at": "2026-05-02T12:00:00Z",
                "verification_hash": "a" * 64,
                "content_hash": "b" * 64,
                "content_id_hash": "c" * 64,
                "oracle_feed": "11111111111111111111111111111115",
            }
        ),
        encoding="utf-8",
    )

    monkeypatch.setattr("sys.argv", ["vow-verify", str(att_file)])
    exit_code = main()
    assert exit_code == 1
    out = json.loads(capsys.readouterr().out)
    assert out["valid"] is False
    assert out["failure_reason"] == "network_mismatch"
    assert "mainnet" in out["rpc_endpoint"]
    assert seen_urls == []

