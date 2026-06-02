"""extension-registry ``Extension`` decoder tests.

Mirrors ``sdk/aas-verifier-ts/__tests__/extension.test.ts``. The primary
fixture is a REAL devnet Extension account (the permanent dogfood extension
root CKsZ... -> agent B9H6..., PDA 4gAYnHAChB6PMroRa6Rv7C2MTkf86R2f6qqe6kbWwvSZ),
captured verbatim as base64 — so this decodes a genuine on-chain record,
network-free.
"""

from __future__ import annotations

import base64

import pytest

from aas_verifier import (
    EXTENSION_ACCOUNT_KIND,
    EXTENSION_MIN_BODY_LEN,
    anchor_discriminator,
    decode_extension,
    resolve_extension_type,
)

# Real devnet account data (full 106 bytes incl. the 8-byte discriminator).
REAL_DEVNET_EXTENSION_B64 = (
    "jbd+KhftCwWoRiwGfwms/EriH89VTKrVNRnA02kf0yLikuVftJPCQ5azt3iMmNnp+2hG"
    "XMiDkChIC/TXJfvXpZ6D8YT57eObAEBLTAAAAAAAyR4fagAAAAD/AAAAAAAAAAAAAAAA"
    "AAAAAA=="
)


def _real_account() -> bytes:
    return base64.b64decode(REAL_DEVNET_EXTENSION_B64)


def test_decodes_real_devnet_record() -> None:
    account = _real_account()
    assert len(account) == 106  # Extension::SPACE

    # Step 4 parity: the first 8 bytes are the Anchor discriminator.
    assert account[:8] == anchor_discriminator(EXTENSION_ACCOUNT_KIND)

    ext = decode_extension(account[8:], EXTENSION_ACCOUNT_KIND)
    assert ext.extender == "CKsZ7ZMLLUzbHUeu2Vm5mjuB8QQi3vfvqvXFdFxT7xmY"
    assert ext.recipient == "B9H6dLnZNrYa6Gkho8TsKo7nRKgpuK7SSYBbHNC4qiY2"
    assert ext.extension_type == 0
    assert resolve_extension_type(ext.extension_type) == "capability_validation"
    assert ext.bond_lamports == 5_000_000
    assert ext.created_at == 1_780_424_393
    assert ext.bump == 255


def test_rejects_wrong_account_kind() -> None:
    with pytest.raises(ValueError, match='only handles account_kind="Extension"'):
        decode_extension(bytes(EXTENSION_MIN_BODY_LEN), "Task")


def test_rejects_short_body() -> None:
    with pytest.raises(ValueError, match="too short"):
        decode_extension(bytes(EXTENSION_MIN_BODY_LEN - 1), EXTENSION_ACCOUNT_KIND)


def test_resolve_extension_type_taxonomy() -> None:
    assert resolve_extension_type(0) == "capability_validation"
    assert resolve_extension_type(1) == "mentorship"
    assert resolve_extension_type(2) == "sponsorship"
    assert resolve_extension_type(3) == "validation"
    assert resolve_extension_type(4) is None
