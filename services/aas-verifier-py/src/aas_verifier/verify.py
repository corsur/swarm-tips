"""AAS v1 verification protocol — Python reference.

Mirrors ``services/aas-verifier-ts/src/verify.ts``. Steps split:

* ``verify_v1_schema`` — pure (steps 1, 6).
* ``verify_v1_on_chain`` — touches RPC (steps 2-5, 7).
* ``verify_v1`` — full 7-step pipeline.

The RPC abstraction is a callable that takes a base58 pubkey and
returns ``(owner_b58, data_bytes) | None``. This keeps the verifier
HTTP-library-agnostic — the CLI in ``cli.py`` ships a default
implementation against ``requests`` + Solana's JSON-RPC.
"""

from __future__ import annotations

from datetime import datetime, timezone
from typing import Any, Callable, Optional, Tuple

from .discriminator import anchor_discriminator
from .schema import check_schema, parse_verified_at_seconds
from .types import AasV1Attestation, ProtocolHandler, Verdict


# RPC abstraction — caller injects. Returns None if the account doesn't
# exist; ``(owner_b58, data_bytes)`` otherwise.
RpcFetcher = Callable[[str], Optional[Tuple[str, bytes]]]


def verify_v1(
    attestation: Any,
    protocol: ProtocolHandler,
    rpc: RpcFetcher,
    rpc_endpoint: str = "<injected>",
) -> Verdict:
    checked_at = _now_rfc3339()

    schema_fail = verify_v1_schema(attestation)
    if schema_fail is not None:
        return _failure(attestation, checked_at, rpc_endpoint, schema_fail)

    on_chain_fail = verify_v1_on_chain(attestation, protocol, rpc)
    if on_chain_fail is not None:
        return _failure(attestation, checked_at, rpc_endpoint, on_chain_fail)

    return {
        "valid": True,
        "attestation": attestation,
        "checked_at": checked_at,
        "rpc_endpoint": rpc_endpoint,
    }


def verify_v1_schema(attestation: Any) -> Optional[str]:
    """Pure step 1 + step 6. Returns ``None`` on success."""
    schema_fail = check_schema(attestation)
    if schema_fail is not None:
        return schema_fail

    a = attestation
    # Step 6 — domain bound. Both are decimal-string u64 (passed step 1).
    if int(a["composite_score"]) > int(a["score_max"]):
        return "score_above_max"
    return None


def verify_v1_on_chain(
    a: AasV1Attestation,
    protocol: ProtocolHandler,
    rpc: RpcFetcher,
) -> Optional[str]:
    # Step 2 — fetch the on-chain account.
    account = rpc(a["account"])
    if account is None:
        return "account_closed"
    owner_b58, data = account

    # Step 3 — owner check.
    if owner_b58 != a["program_id"]:
        return "owner_mismatch"

    # Step 4 — Anchor discriminator check.
    if len(data) < 8:
        return "discriminator_mismatch"
    expected_disc = anchor_discriminator(a["account_kind"])
    if data[:8] != expected_disc:
        return "discriminator_mismatch"

    # Step 5 — field equality.
    decoded = protocol.decode(data[8:], a["account_kind"])

    if str(decoded.task_id) != a["task_id"]:
        return "field_mismatch:task_id"
    if decoded.client != a["client"]:
        return "field_mismatch:client"
    if decoded.agent != a["agent"]:
        return "field_mismatch:agent"
    if str(decoded.composite_score) != a["composite_score"]:
        return "field_mismatch:composite_score"
    if decoded.verification_hash != a["verification_hash"]:
        return "field_mismatch:verification_hash"
    if decoded.content_hash != a["content_hash"]:
        return "field_mismatch:content_hash"
    if decoded.content_id_hash != a["content_id_hash"]:
        return "field_mismatch:content_id_hash"

    # verified_at: chain stores i64 unix seconds; wire is RFC 3339 with
    # no fractional seconds. Convert and compare with ±1 second
    # tolerance (per spec §4 step 5).
    wire_seconds = parse_verified_at_seconds(a["verified_at"])
    chain_seconds = int(decoded.verified_at)
    if abs(wire_seconds - chain_seconds) > 1:
        return "field_mismatch:verified_at"

    # Step 5 (state) — map decoded u8 to the protocol's published name.
    resolved = protocol.resolve_state(decoded.state, a["account_kind"])
    if resolved is None or resolved != a["state"]:
        return "field_mismatch:state"

    # Step 7 — state validity. Spec §4 step 7 says protocols publish
    # which on-chain states are "valid for attestation"; the verifier
    # delegates to the protocol handler's ``valid_states`` to avoid
    # hardcoding any one protocol's policy. Shillbot returns
    # ``("verified",)``; other protocols can return larger tuples
    # without forking this verifier.
    valid = protocol.valid_states(a["account_kind"])
    if a["state"] not in valid:
        return "state_invalid"

    return None


def _failure(
    attestation: Any, checked_at: str, rpc_endpoint: str, reason: str
) -> Verdict:
    return {
        "valid": False,
        "attestation": attestation,
        "checked_at": checked_at,
        "rpc_endpoint": rpc_endpoint,
        "failure_reason": reason,
    }


def _now_rfc3339() -> str:
    return datetime.now(tz=timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
