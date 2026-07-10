"""VOW v1 verifier — pure-step unit tests.

Mirrors ``sdk/vow-verifier-ts/__tests__/verify.test.ts`` for the
schema and on-chain steps. Step 2-5 are exercised against a stub RPC
fetcher (no network).
"""

from __future__ import annotations

from typing import Any, Dict, Optional, Tuple

import pytest

from vow_verifier import (
    SHILLBOT_PROTOCOL,
    anchor_discriminator,
    check_schema,
    verify_v1_on_chain,
    verify_v1_schema,
)


def _fixture(**overrides: Any) -> Dict[str, Any]:
    base: Dict[str, Any] = {
        "version": "vow/v1",
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
    assert check_schema(_fixture(version="vow/v0")) == "schema_invalid:version"


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


# ----- happy path + step 5 / step 7 negatives ----------------------
#
# Builds 315 bytes of synthetic on-chain Task account data that matches
# the wire-format attestation, then mutates one field at a time to
# confirm each ``field_mismatch:<field>`` reason is reachable. Without
# this the on-chain happy path is uncovered — only the negatives at
# steps 2-4 are exercised above.


import struct
from datetime import datetime, timezone

import base58

from vow_verifier import ProtocolHandler, verify_v1
from vow_verifier.types import DecodedAccount


_HAPPY_TASK_ID = 42
_HAPPY_VERIFIED_AT_SECS = 1_746_201_600
_HAPPY_VERIFIED_AT_ISO = (
    datetime.fromtimestamp(_HAPPY_VERIFIED_AT_SECS, tz=timezone.utc)
    .strftime("%Y-%m-%dT%H:%M:%SZ")
)
_HAPPY_COMPOSITE_SCORE = 850_000
_HAPPY_VERIFY_HASH = "a" * 64
_HAPPY_CONTENT_HASH = "b" * 64
_HAPPY_CONTENT_ID_HASH = "c" * 64

_SHILLBOT_PROGRAM_ID = "2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi"
_HAPPY_ACCOUNT = "11111111111111111111111111111112"
_HAPPY_CLIENT = "11111111111111111111111111111113"
_HAPPY_AGENT = "11111111111111111111111111111114"


def _build_happy_account_bytes(state_byte: int = 3) -> bytes:
    """Construct 315 bytes (discriminator + 307-byte body) matching the
    happy-path attestation. ``state_byte`` defaults to 3 (Verified);
    pass 7 to test a state-invalid scenario.

    Layout per ``decoders/shillbot.py``."""
    from vow_verifier.discriminator import anchor_discriminator

    body = bytearray(307)
    struct.pack_into("<Q", body, 0, _HAPPY_TASK_ID)
    body[8:40] = base58.b58decode(_HAPPY_CLIENT)
    body[40:72] = base58.b58decode(_HAPPY_AGENT)
    body[72] = state_byte
    body[73] = 0  # platform = YouTube
    # 74-82: escrow_lamports (zero)
    body[82:114] = bytes.fromhex(_HAPPY_CONTENT_HASH)
    body[114:146] = bytes.fromhex(_HAPPY_CONTENT_ID_HASH)
    # 146-162: task_nonce (zero)
    struct.pack_into("<Q", body, 162, _HAPPY_COMPOSITE_SCORE)
    # 170-226: payment_amount, fee_amount, deadline, submit_margin,
    # claim_buffer, created_at, submitted_at (zero)
    struct.pack_into("<q", body, 226, _HAPPY_VERIFIED_AT_SECS)
    # 234-254: challenge_deadline + three u32 overrides (zero)
    body[254:286] = bytes.fromhex(_HAPPY_VERIFY_HASH)
    # 286-306: _reserved (zero)
    body[306] = 254  # bump

    return anchor_discriminator("Task") + bytes(body)


def _happy_attestation(**overrides: Any) -> Dict[str, Any]:
    base = {
        "version": "vow/v1",
        "network": "mainnet",
        "program_id": _SHILLBOT_PROGRAM_ID,
        "account": _HAPPY_ACCOUNT,
        "account_kind": "Task",
        "task_id": str(_HAPPY_TASK_ID),
        "client": _HAPPY_CLIENT,
        "agent": _HAPPY_AGENT,
        "state": "verified",
        "platform": 0,
        "composite_score": str(_HAPPY_COMPOSITE_SCORE),
        "score_max": "1000000",
        "verified_at": _HAPPY_VERIFIED_AT_ISO,
        "verification_hash": _HAPPY_VERIFY_HASH,
        "content_hash": _HAPPY_CONTENT_HASH,
        "content_id_hash": _HAPPY_CONTENT_ID_HASH,
        "oracle_feed": "11111111111111111111111111111115",
    }
    base.update(overrides)
    return base


def _happy_rpc(state_byte: int = 3):
    data = _build_happy_account_bytes(state_byte=state_byte)
    return _stub_rpc((_SHILLBOT_PROGRAM_ID, data))


def test_happy_path_returns_valid_true():
    verdict = verify_v1(_happy_attestation(), SHILLBOT_PROTOCOL, _happy_rpc())
    assert verdict["valid"] is True


@pytest.mark.parametrize(
    "field,wire_value,reason",
    [
        ("task_id", "43", "field_mismatch:task_id"),
        ("client", "11111111111111111111111111111119", "field_mismatch:client"),
        ("agent", "11111111111111111111111111111119", "field_mismatch:agent"),
        ("composite_score", "850001", "field_mismatch:composite_score"),
        ("verification_hash", "d" * 64, "field_mismatch:verification_hash"),
        ("content_hash", "d" * 64, "field_mismatch:content_hash"),
        ("content_id_hash", "d" * 64, "field_mismatch:content_id_hash"),
    ],
)
def test_field_mismatch_reasons_are_each_reachable(field, wire_value, reason):
    verdict = verify_v1(
        _happy_attestation(**{field: wire_value}),
        SHILLBOT_PROTOCOL,
        _happy_rpc(),
    )
    assert verdict["valid"] is False
    assert verdict["failure_reason"] == reason


def test_field_mismatch_verified_at_when_drift_above_one_second():
    drifted_secs = _HAPPY_VERIFIED_AT_SECS + 5
    drifted_iso = (
        datetime.fromtimestamp(drifted_secs, tz=timezone.utc)
        .strftime("%Y-%m-%dT%H:%M:%SZ")
    )
    verdict = verify_v1(
        _happy_attestation(verified_at=drifted_iso),
        SHILLBOT_PROTOCOL,
        _happy_rpc(),
    )
    assert verdict["valid"] is False
    assert verdict["failure_reason"] == "field_mismatch:verified_at"


def test_verified_at_one_second_drift_accepted():
    drifted_secs = _HAPPY_VERIFIED_AT_SECS + 1
    drifted_iso = (
        datetime.fromtimestamp(drifted_secs, tz=timezone.utc)
        .strftime("%Y-%m-%dT%H:%M:%SZ")
    )
    verdict = verify_v1(
        _happy_attestation(verified_at=drifted_iso),
        SHILLBOT_PROTOCOL,
        _happy_rpc(),
    )
    assert verdict["valid"] is True


def test_field_mismatch_state_when_wire_disagrees_with_resolved_chain_state():
    verdict = verify_v1(
        _happy_attestation(state="approved"),
        SHILLBOT_PROTOCOL,
        _happy_rpc(),
    )
    assert verdict["valid"] is False
    assert verdict["failure_reason"] == "field_mismatch:state"


def test_state_invalid_when_protocol_doesnt_allow_state():
    # Wire claims "approved" AND on-chain state byte is 7 (Approved). Step 5
    # passes; step 7 rejects because Shillbot's valid_states is ("verified",).
    verdict = verify_v1(
        _happy_attestation(state="approved"),
        SHILLBOT_PROTOCOL,
        _happy_rpc(state_byte=7),
    )
    assert verdict["valid"] is False
    assert verdict["failure_reason"] == "state_invalid"


def test_custom_protocol_with_multi_state_valid_states_accepts_what_shillbot_rejects():
    # validStates extension point isn't Shillbot-locked.
    liberal = ProtocolHandler(
        decode=SHILLBOT_PROTOCOL.decode,
        resolve_state=SHILLBOT_PROTOCOL.resolve_state,
        valid_states=lambda kind: ("verified", "approved"),
    )
    verdict = verify_v1(
        _happy_attestation(state="approved"),
        liberal,
        _happy_rpc(state_byte=7),
    )
    assert verdict["valid"] is True


# ----- optional-field schema validation -----------------------------


def test_schema_rejects_verifier_instructions_wrong_type():
    a = _happy_attestation()
    a["verifier_instructions"] = 42  # number instead of string
    assert check_schema(a) == "schema_invalid:verifier_instructions"


def test_schema_accepts_string_verifier_instructions():
    a = _happy_attestation()
    a["verifier_instructions"] = "re-read on-chain"
    assert check_schema(a) is None
