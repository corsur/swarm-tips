"""AAS v1 wire format and verifier types. Mirrors the TS reference."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable, Dict, Literal, Optional, TypedDict, Union


class AasV1Attestation(TypedDict, total=False):
    """v1 wire format. Mirrors ``docs/specs/aas-v1.md`` §3.

    Required fields are listed here; ``verifier_instructions`` and
    ``extensions`` are optional. ``total=False`` keeps ``TypedDict``
    happy with the optional fields; presence of required fields is
    enforced by ``check_schema``.
    """

    version: Literal["aas/v1"]
    network: Literal["mainnet", "devnet"]
    program_id: str
    account: str
    account_kind: str
    task_id: str
    client: str
    agent: str
    state: str
    platform: int
    composite_score: str
    score_max: str
    verified_at: str
    verification_hash: str
    content_hash: str
    content_id_hash: str
    oracle_feed: Optional[str]
    verifier_instructions: str
    extensions: Dict[str, Any]


@dataclass(frozen=True)
class DecodedAccount:
    """Subset of the on-chain account body the verifier needs to compare."""

    task_id: int
    client: str
    agent: str
    state: int
    composite_score: int
    verified_at: int
    verification_hash: str
    content_hash: str
    content_id_hash: str


# Pluggable per-protocol decoders. ``data`` is the account body AFTER the
# 8-byte Anchor discriminator; ``account_kind`` is for routing.
AccountDecoder = Callable[[bytes, str], DecodedAccount]
StateResolver = Callable[[int, str], Optional[str]]


@dataclass(frozen=True)
class ProtocolHandler:
    decode: AccountDecoder
    resolve_state: StateResolver


# Verdict shape. ``failure_reason`` is a string from the closed taxonomy
# in spec §4 (e.g. ``"account_closed"``, ``"field_mismatch:task_id"``,
# ``"schema_invalid:network"``). Verifiers MAY return additional
# context fields; consumers SHOULD treat unknown keys as opaque.
Verdict = Dict[str, Union[bool, str, Dict[str, Any]]]
