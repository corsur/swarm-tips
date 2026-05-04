"""Step 1 of the AAS v1 verifier — pure schema check.

Implements the well-formed-per-type table from
``docs/specs/aas-v1.md`` §4. Returns the first failure reason
encountered, or ``None`` if the attestation passes.
"""

from __future__ import annotations

import re
from datetime import datetime, timezone
from typing import Any, Optional

import base58


U64_MAX = (1 << 64) - 1

# Anchor pattern: YYYY-MM-DDTHH:MM:SS{Z|+00:00}. No fractional seconds
# (per spec §3 — on-chain source is i64 unix seconds).
_RFC3339_NO_FRACTION = re.compile(
    r"^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(Z|\+00:00)$"
)
_DECIMAL_U64 = re.compile(r"^[0-9]+$")
_HEX32 = re.compile(r"^[0-9a-f]{64}$")


def check_schema(input_value: Any) -> Optional[str]:
    """Return the first ``schema_invalid:<field>`` reason, or ``None``."""
    if not isinstance(input_value, dict):
        return "schema_invalid:<root>"

    a = input_value

    if a.get("version") != "aas/v1":
        return "schema_invalid:version"

    if not _is_pubkey(a.get("program_id")):
        return "schema_invalid:program_id"
    if not _is_pubkey(a.get("account")):
        return "schema_invalid:account"
    if not _is_pubkey(a.get("client")):
        return "schema_invalid:client"
    if not _is_pubkey(a.get("agent")):
        return "schema_invalid:agent"

    oracle_feed = a.get("oracle_feed", "missing")
    if oracle_feed != "missing" and oracle_feed is not None and not _is_pubkey(oracle_feed):
        return "schema_invalid:oracle_feed"
    if oracle_feed == "missing":
        return "schema_invalid:oracle_feed"

    if a.get("network") not in ("mainnet", "devnet"):
        return "schema_invalid:network"

    if not _is_u8(a.get("platform")):
        return "schema_invalid:platform"

    if not _is_decimal_u64(a.get("task_id")):
        return "schema_invalid:task_id"
    if not _is_decimal_u64(a.get("composite_score")):
        return "schema_invalid:composite_score"
    if not _is_decimal_u64(a.get("score_max")):
        return "schema_invalid:score_max"

    if not _is_hex32(a.get("verification_hash")):
        return "schema_invalid:verification_hash"
    if not _is_hex32(a.get("content_hash")):
        return "schema_invalid:content_hash"
    if not _is_hex32(a.get("content_id_hash")):
        return "schema_invalid:content_id_hash"

    if not _is_enum_string(a.get("state")):
        return "schema_invalid:state"
    if not _is_enum_string(a.get("account_kind")):
        return "schema_invalid:account_kind"

    if not _is_rfc3339_no_fraction(a.get("verified_at")):
        return "schema_invalid:verified_at"

    extensions = a.get("extensions")
    if extensions is not None and not isinstance(extensions, dict):
        return "schema_invalid:extensions"

    return None


def _is_pubkey(v: Any) -> bool:
    """Base58, decodes to exactly 32 bytes."""
    if not isinstance(v, str) or not v:
        return False
    try:
        return len(base58.b58decode(v)) == 32
    except Exception:
        return False


def _is_u8(v: Any) -> bool:
    return isinstance(v, int) and not isinstance(v, bool) and 0 <= v <= 255


def _is_decimal_u64(v: Any) -> bool:
    if not isinstance(v, str) or not _DECIMAL_U64.fullmatch(v):
        return False
    if len(v) > 1 and v.startswith("0"):
        return False
    try:
        n = int(v)
    except ValueError:
        return False
    return 0 <= n <= U64_MAX


def _is_hex32(v: Any) -> bool:
    return isinstance(v, str) and bool(_HEX32.fullmatch(v))


def _is_enum_string(v: Any) -> bool:
    return isinstance(v, str) and len(v) > 0 and not any(ch.isspace() for ch in v)


def _is_rfc3339_no_fraction(v: Any) -> bool:
    if not isinstance(v, str):
        return False
    if not _RFC3339_NO_FRACTION.fullmatch(v):
        return False
    # Round-trip through datetime to confirm the components describe a
    # real moment (rejects e.g. "2026-13-32T25:61:99Z").
    s = v.replace("Z", "+00:00")
    try:
        datetime.fromisoformat(s)
    except ValueError:
        return False
    return True


def parse_verified_at_seconds(v: str) -> int:
    """Parse the wire ``verified_at`` to unix seconds. Caller is
    responsible for having already passed schema check."""
    s = v.replace("Z", "+00:00")
    dt = datetime.fromisoformat(s).astimezone(timezone.utc)
    return int(dt.timestamp())
