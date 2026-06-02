"""extension-registry ``Extension`` account decoder — the mund-creanc-witer
obligation edge. Mirrors ``programs/extension-registry/src/state/extension.rs``
and the TS reference at ``sdk/aas-verifier-ts/src/decoders/extension.ts``
(either is authoritative; both track the same on-chain layout).

Unlike Shillbot's ``Task``, an ``Extension`` has no on-chain state enum: the
account exists only while the obligation is ACTIVE — ``attest_return_substance``
(success) and ``default_extension`` (slash) both CLOSE it, emitting the terminal
event. A present account is "active"; a closed one is terminal and un-attestable
(verifier ``account_closed``), exactly like a finalized Task.

This is a typed decoder + type helper for downstream consumers, NOT a drop-in
``ProtocolHandler``: that interface's ``DecodedAccount`` is Task-shaped, and the
verifier's step-5 field equality compares Task fields. Verifying an Extension
*attestation* end-to-end needs the verifier core generalized per ``account_kind``
(tracked separately).

Body offsets (after the 8-byte Anchor discriminator):

    off  size  field
      0    32  extender        (Pubkey)
     32    32  recipient       (Pubkey)
     64     1  extension_type  (u8)
     65     8  bond_lamports   (u64 LE)
     73     8  created_at      (i64 LE, unix seconds)
     81     1  bump
     82    16  _reserved       (skipped)

Body = 98 bytes; full account (with discriminator) = 106 = ``Extension::SPACE``.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass
from typing import Optional

import base58

#: Anchor account name (-> discriminator ``sha256("account:Extension")[0:8]``).
EXTENSION_ACCOUNT_KIND = "Extension"

#: Minimum body length carrying every decoded field (through ``bump``).
EXTENSION_MIN_BODY_LEN = 82

#: A live Extension has no state byte — existence == active.
EXTENSION_ACTIVE_STATE = "active"

_EXTENSION_TYPE_NAMES = {
    0: "capability_validation",
    1: "mentorship",
    2: "sponsorship",
    3: "validation",
}


@dataclass(frozen=True)
class ExtensionRecord:
    extender: str
    recipient: str
    extension_type: int
    bond_lamports: int
    created_at: int
    bump: int


def decode_extension(data: bytes, account_kind: str) -> ExtensionRecord:
    """Decode an ``Extension`` account body (bytes AFTER the discriminator)."""
    if account_kind != EXTENSION_ACCOUNT_KIND:
        raise ValueError(
            f'decode_extension only handles account_kind="Extension", got "{account_kind}"'
        )
    if len(data) < EXTENSION_MIN_BODY_LEN:
        raise ValueError(
            f"Extension body too short: {len(data)} bytes "
            f"(need >= {EXTENSION_MIN_BODY_LEN})"
        )
    return ExtensionRecord(
        extender=base58.b58encode(data[0:32]).decode("ascii"),
        recipient=base58.b58encode(data[32:64]).decode("ascii"),
        extension_type=data[64],
        bond_lamports=struct.unpack_from("<Q", data, 65)[0],
        created_at=struct.unpack_from("<q", data, 73)[0],
        bump=data[81],
    )


def resolve_extension_type(extension_type: int) -> Optional[str]:
    """Extension-type discriminant -> name (constants.rs taxonomy). MVP records
    only CapabilityValidation (0). Returns ``None`` for unknown discriminants."""
    return _EXTENSION_TYPE_NAMES.get(extension_type)
