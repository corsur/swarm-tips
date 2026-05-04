"""AAS v1 verifier — pure-step unit tests.

Mirrors ``services/aas-verifier-ts/__tests__/verify.test.ts`` for the
schema and on-chain steps. Step 2-5 are exercised against a stub RPC
fetcher (no network).
"""

from __future__ import annotations

from typing import Any, Dict, Optional, Tuple

import pytest

from aas_verifier import (
    SHILLBOT_PROTOCOL,
    anchor_discriminator,
    check_schema,
    verify_v1_on_chain,
    verify_v1_schema,
)


def _fixture(**overrides: Any) -> Dict[str, Any]:
    base: Dict[str, Any] = {
        "version": "aas/v1",
        "network": "mainnet",
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
    base.update(overrides)
    return base


# ----- step 1: schema check ----------------------------------------


def test_schema_accepts_well_formed():
    assert check_schema(_fixture()) is None


def test_schema_rejects_wrong_version():
    assert check_schema(_fixture(version="aas/v0")) == "schema_invalid:version"


def test_schema_rejects_non_base58_pubkey():
    assert (
        check_schema(_fixture(program_id="not-base58!!"))
        == "schema_invalid:program_id"
    )


def test_schema_accepts_oracle_feed_null():
    assert check_schema(_fixture(oracle_feed=None)) is None


def test_schema_rejects_oracle_feed_empty_string():
    assert check_schema(_fixture(oracle_feed="")) == "schema_invalid:oracle_feed"


def test_schema_rejects_unknown_network():
    assert check_schema(_fixture(network="testnet")) == "schema_invalid:network"


def test_schema_rejects_platform_out_of_range():
    assert check_schema(_fixture(platform=256)) == "schema_invalid:platform"
    assert check_schema(_fixture(platform=-1)) == "schema_invalid:platform"


def test_schema_rejects_task_id_leading_zero():
    assert check_schema(_fixture(task_id="042")) == "schema_invalid:task_id"


def test_schema_accepts_task_id_zero():
    assert check_schema(_fixture(task_id="0")) is None


def test_schema_rejects_task_id_overflow():
    # u64 max + 1
    assert (
        check_schema(_fixture(task_id="18446744073709551616"))
        == "schema_invalid:task_id"
    )


def test_schema_rejects_uppercase_hex():
    assert (
        check_schema(_fixture(verification_hash="A" * 64))
        == "schema_invalid:verification_hash"
    )


def test_schema_rejects_rfc3339_with_fraction():
    assert (
        check_schema(_fixture(verified_at="2026-05-02T12:00:00.123Z"))
        == "schema_invalid:verified_at"
    )


def test_schema_accepts_both_z_and_offset_form():
    assert (
        check_schema(_fixture(verified_at="2026-05-02T12:00:00+00:00"))
        is None
    )


# ----- step 6: domain bound ----------------------------------------


def test_domain_bound_rejects_score_above_max():
    assert (
        verify_v1_schema(_fixture(composite_score="1000001", score_max="1000000"))
        == "score_above_max"
    )


def test_domain_bound_accepts_score_equal_max():
    assert (
        verify_v1_schema(_fixture(composite_score="1000000", score_max="1000000"))
        is None
    )


# ----- discriminator helper ----------------------------------------


def test_discriminator_is_deterministic():
    a = anchor_discriminator("Task")
    b = anchor_discriminator("Task")
    assert len(a) == 8
    assert a == b


# ----- step 2-5+7 on-chain (mocked RPC) -----------------------------


def _stub_rpc(account: Optional[Tuple[str, bytes]]):
    def fetch(_b58: str):
        return account

    return fetch


def test_returns_account_closed_when_rpc_says_none():
    res = verify_v1_on_chain(_fixture(), SHILLBOT_PROTOCOL, _stub_rpc(None))
    assert res == "account_closed"


def test_returns_owner_mismatch_when_owner_differs():
    disc = anchor_discriminator("Task")
    body = bytes(307)
    data = disc + body
    res = verify_v1_on_chain(
        _fixture(),
        SHILLBOT_PROTOCOL,
        _stub_rpc(("11111111111111111111111111111111", data)),
    )
    assert res == "owner_mismatch"


def test_returns_discriminator_mismatch_when_first_8_bytes_wrong():
    data = bytes(315)  # all zeros — discriminator won't match sha256("account:Task")[0..8]
    res = verify_v1_on_chain(
        _fixture(),
        SHILLBOT_PROTOCOL,
        _stub_rpc(("2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi", data)),
    )
    assert res == "discriminator_mismatch"
