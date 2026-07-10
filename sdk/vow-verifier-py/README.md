# `vow-verifier` (Python)

Reference implementation of the VOW v1 verification protocol from
`docs/specs/vow-v1.md`. Non-normative — the spec is authoritative.

## Install

```sh
pip install vow-verifier
```

## Use

```python
from vow_verifier import verify_v1, SHILLBOT_PROTOCOL
from vow_verifier.cli import make_solana_rpc_fetcher

rpc = make_solana_rpc_fetcher("https://api.mainnet-beta.solana.com")
verdict = verify_v1(attestation_dict, SHILLBOT_PROTOCOL, rpc)
if verdict["valid"]:
    print("verified:", attestation_dict["composite_score"])
else:
    print("rejected:", verdict["failure_reason"])
```

For non-Shillbot protocols, supply your own `ProtocolHandler`:

```python
from vow_verifier import ProtocolHandler, verify_v1

my_protocol = ProtocolHandler(decode=my_decoder, resolve_state=my_resolver)
verify_v1(attestation, my_protocol, rpc)
```

## CLI

```sh
vow-verify path/to/attestation.json
vow-verify - < attestation.json    # stdin
vow-verify path/to/a.json --rpc https://my-rpc.example
```

Exit codes: 0 = valid, 1 = invalid (verdict still printed), 2 = usage / read error.

## Conformance

Implements all 7 steps of `docs/specs/vow-v1.md` §4 with the closed
failure-reason taxonomy. Bug reports welcome — if a verdict
disagrees with the spec, please file an issue.
