"""AAS v1 reference verifier (Python).

Public API mirrors the TypeScript reference at
``services/aas-verifier-ts``. Spec lives at
``docs/specs/aas-v1.md``.
"""

from .types import (
    AasV1Attestation,
    DecodedAccount,
    ProtocolHandler,
    Verdict,
)
from .schema import check_schema
from .verify import (
    verify_v1,
    verify_v1_schema,
    verify_v1_on_chain,
)
from .discriminator import anchor_discriminator
from .decoders.shillbot import (
    decode_shillbot_task,
    resolve_shillbot_state,
    SHILLBOT_PROTOCOL,
)

__all__ = [
    "AasV1Attestation",
    "DecodedAccount",
    "ProtocolHandler",
    "Verdict",
    "check_schema",
    "verify_v1",
    "verify_v1_schema",
    "verify_v1_on_chain",
    "anchor_discriminator",
    "decode_shillbot_task",
    "resolve_shillbot_state",
    "SHILLBOT_PROTOCOL",
]
