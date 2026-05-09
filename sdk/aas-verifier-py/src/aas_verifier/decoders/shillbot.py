"""Shillbot ``Task`` account decoder + state resolver. Mirrors the
canonical layout in ``programs/shillbot/src/state/task.rs``.

Field offsets within the body (post-discriminator) — see the TS
reference at ``sdk/aas-verifier-ts/src/decoders/shillbot.ts``
for the full annotated map. Both implementations track the same
on-chain layout; either is authoritative.
"""

from __future__ import annotations

import struct
from typing import Optional

import base58

from ..types import DecodedAccount, ProtocolHandler


def decode_shillbot_task(data: bytes, account_kind: str) -> DecodedAccount:
    if account_kind != "Task":
        raise ValueError(
            f'decode_shillbot_task only handles account_kind="Task", got "{account_kind}"'
        )
    if len(data) < 286:
        raise ValueError(
            f"Shillbot Task body too short: {len(data)} bytes "
            f"(need >= 286 for the AAS-relevant fields)"
        )

    task_id = struct.unpack_from("<Q", data, 0)[0]
    client = base58.b58encode(data[8:40]).decode("ascii")
    agent = base58.b58encode(data[40:72]).decode("ascii")
    state = data[72]
    content_hash = data[82:114].hex()
    content_id_hash = data[114:146].hex()
    composite_score = struct.unpack_from("<Q", data, 162)[0]
    verified_at = struct.unpack_from("<q", data, 226)[0]
    verification_hash = data[254:286].hex()

    return DecodedAccount(
        task_id=task_id,
        client=client,
        agent=agent,
        state=state,
        composite_score=composite_score,
        verified_at=verified_at,
        verification_hash=verification_hash,
        content_hash=content_hash,
        content_id_hash=content_id_hash,
    )


_SHILLBOT_STATE_NAMES = {
    0: "open",
    1: "claimed",
    2: "submitted",
    3: "verified",
    4: "finalized",
    5: "disputed",
    6: "resolved",
    7: "approved",
}


def resolve_shillbot_state(state: int, account_kind: str) -> Optional[str]:
    if account_kind != "Task":
        return None
    return _SHILLBOT_STATE_NAMES.get(state)


def shillbot_valid_states(account_kind: str):
    """Shillbot's "states valid for attestation" policy. Per
    ``docs/specs/aas-v1.md`` §6 capture window: only Verified is
    attestable — finalize closes the on-chain account."""
    if account_kind != "Task":
        return ()
    return ("verified",)


SHILLBOT_PROTOCOL = ProtocolHandler(
    decode=decode_shillbot_task,
    resolve_state=resolve_shillbot_state,
    valid_states=shillbot_valid_states,
)
