# `@swarm-tips/vow-verifier` (TypeScript)

Reference implementation of the VOW v1 verification protocol from
`docs/specs/vow-v1.md`. Non-normative — the spec is authoritative.

## Install

```sh
pnpm add @swarm-tips/vow-verifier
```

## Use

```ts
import { Connection } from "@solana/web3.js";
import { verifyV1, shillbotProtocol } from "@swarm-tips/vow-verifier";

const rpc = new Connection("https://api.mainnet-beta.solana.com", "confirmed");
const attestation = JSON.parse(jsonString);
const verdict = await verifyV1(attestation, shillbotProtocol, rpc);

if (verdict.valid) {
  console.log("verified:", verdict.attestation.composite_score);
} else {
  console.log("rejected:", verdict.failure_reason);
}
```

For non-Shillbot protocols, supply your own `ProtocolHandler`:

```ts
import { ProtocolHandler, verifyV1 } from "@swarm-tips/vow-verifier";

const myProtocol: ProtocolHandler = {
  decode: (bytes, accountKind) => { /* ... */ },
  resolveState: (state, accountKind) => { /* ... */ },
};
await verifyV1(attestation, myProtocol, rpc);
```

## CLI

```sh
vow-verify path/to/attestation.json
vow-verify - < attestation.json    # stdin
vow-verify path/to/a.json --rpc https://my-rpc.example
```

Exit codes: 0 = valid, 1 = invalid (verdict still printed), 2 = usage / read error.

## Conformance

Implements all 7 steps of `docs/specs/vow-v1.md` §4 with the
closed failure-reason taxonomy. Bug reports welcome — if a verdict
disagrees with the spec, please file an issue.
