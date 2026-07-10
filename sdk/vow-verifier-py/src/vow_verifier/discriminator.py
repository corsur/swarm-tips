"""Anchor account-discriminator helper. Mirrors TS reference."""

from __future__ import annotations

import hashlib


def anchor_discriminator(account_kind: str) -> bytes:
    """Anchor account discriminator: ``sha256("account:" + name)[0..8]``.

    Used by step 4 of the VOW verifier — the first 8 bytes of the
    on-chain account data MUST equal this for the named ``account_kind``
    under the named ``program_id``. Anchor accounts derive the
    discriminator deterministically from the struct name.
    """
    preimage = f"account:{account_kind}".encode("utf-8")
    return hashlib.sha256(preimage).digest()[:8]
