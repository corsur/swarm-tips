# AAS v1 — Agent Attestation Standard (RFC)

**Status:** RFC, draft 1 · 2026-05-02
**Editor:** swarm.tips DAO (`corsur/swarm-tips`)
**Implements:** the wire format `version: "aas/v1"`, superseding the
transitional `"aas/v0"` shipped by Shillbot in commit
[`808ec1d`](../../) (orchestrator) and [`0971ec7`](../../) (MCP server).
**Reference verifiers:** `swarm-tips-repo/services/aas-verifier-ts`
and `aas-verifier-py` (planned, task #18).

---

## 1. Motivation

An AI agent that completes oracle-verified work on one platform (Shillbot
today; other agent-work protocols tomorrow) needs a portable proof it
can hand to a third party — a reputation system, a hiring platform, a
client that wants to cherry-pick high-scoring agents. Today every
platform has its own opaque scoring API; cross-platform reputation
requires N×M integrations.

AAS solves this with a uniform JSON wire format and a verification
recipe that does not depend on trusting the platform that emitted the
attestation. The only trust assumption is the on-chain protocol that
escrowed the work and verified the score — exactly the trust model the
agent already accepted when they took the task.

**Non-goals.** AAS is not a credential format (W3C VC), not a token
standard (ERC-721/SPL token), and not a reputation aggregation algorithm
(EigenTrust). It is the wire-level proof primitive those higher-level
systems can be built on.

---

## 2. Scope

**v1 covers:** Solana-anchored work attestations where the platform's
on-chain state is the source of truth. The reference platform is
Shillbot; the spec is platform-agnostic in shape but pins Solana
conventions (base58 pubkeys, u64 program-defined IDs, sha256 content
hashes).

**v1 does NOT cover:**
- Cross-chain attestations (Ethereum, Bitcoin, etc.). v2 candidate.
- Agent-emitted attestations not anchored to escrowed work.
- Multi-task aggregate proofs ("agent X completed 100 tasks averaging
  0.85"). v2 candidate.
- Revocation. The on-chain account being closed/expired is the
  revocation signal; v1 does not define an alternate mechanism.

---

## 3. Wire format

**Content-Type:** `application/json`. Field ordering MUST be
serialization-stable but is not significant for verification (verifiers
parse JSON, not bytes).

**Required fields (all present in every v1 attestation):**

| Field | Type | Description |
|---|---|---|
| `version` | string | Exactly `"aas/v1"`. Verifiers MUST reject any other value. |
| `network` | string | Solana cluster the on-chain account lives on. Pin: `"mainnet"` or `"devnet"`. Verifier uses this to pick the RPC endpoint. |
| `program_id` | string | Base58 pubkey of the protocol that escrowed and verified the work. Verifier uses this to confirm account ownership when re-reading. |
| `account` | string | Base58 PDA address of the on-chain account that holds the verification result. Verifier MUST re-read this account from `network` and confirm fields below. |
| `account_kind` | string | Anchor account discriminator name (e.g. `"Task"`). Lets a verifier disambiguate when one program emits multiple account types. |
| `task_id` | string | Decimal string of the on-chain u64. **String, not number** — JS `Number.MAX_SAFE_INTEGER` truncates after 2^53-1. (v0 shipped this as a JSON number; v1 fixes that.) |
| `client` | string | Base58 pubkey of the entity that escrowed the work. |
| `agent` | string | Base58 pubkey of the entity that performed the work. |
| `state` | string | Lower-case state name, drawn from a closed enum the protocol publishes. For Shillbot, only `"verified"` is valid in v1 (see §6). |
| `platform` | u8 | Platform discriminant (e.g. 0 = YouTube, 3 = X). Protocol-defined. |
| `composite_score` | string | Decimal string of the fixed-point score. Same string-not-number rationale as `task_id`. |
| `score_max` | string | Decimal string of the score's maximum value. Combined with `composite_score`, defines the score's domain (e.g. `"850000"` / `"1000000"`). |
| `verified_at` | string | RFC 3339 timestamp the on-chain `verified_at` field carries (typically the slot timestamp at oracle attestation). |
| `verification_hash` | string | Hex-encoded 32 bytes from on-chain. The protocol's binding between `task_id` and `composite_score`. |
| `content_hash` | string | Hex-encoded 32 bytes — sha256 of the off-chain campaign brief. |
| `content_id_hash` | string | Hex-encoded 32 bytes — sha256 of the submitted content identifier. |
| `oracle_feed` | string \| null | Base58 pubkey of the oracle feed account that posted the score (e.g. Switchboard pull-feed for Shillbot). `null` if the protocol uses an in-house oracle without a separate feed account. |

**Optional fields:**

| Field | Type | Description |
|---|---|---|
| `verifier_instructions` | string | Plain-English instructions for a human reading the JSON. Verifiers MUST NOT base verification on this field's contents. |
| `extensions` | object | Protocol-specific extension payload. Implementations MAY add fields here without breaking v1 conformance. Verifiers MUST tolerate (preserve, ignore for verification) unknown keys inside this object. |

---

## 4. Verification protocol

A v1 verifier accepts an attestation `A` iff ALL of the following hold:

1. **Schema check.** Every required field in §3 is present and well-formed.
   `version == "aas/v1"`. Reject otherwise.

2. **On-chain read.** Connect to the named `network`, fetch the account
   at `A.account`. The account MUST exist; if it doesn't, reject with
   `account_closed` (the attestation has aged out — see §6 capture
   window).

3. **Owner check.** The fetched account's `owner` MUST equal `A.program_id`.
   Reject otherwise — the attestation claims an account that isn't owned
   by the protocol it names.

4. **Discriminator check.** The first 8 bytes of the account data MUST
   match the Anchor discriminator for `A.account_kind` under
   `A.program_id`. (Anchor's discriminator is `sha256("account:" + name)[0..8]`.)
   Reject otherwise.

5. **Field equality.** Deserialize the account under
   `A.account_kind` and confirm:
   - `task_id` (u64) decimal-equals `A.task_id`
   - `client` (Pubkey) base58-equals `A.client`
   - `agent` (Pubkey) base58-equals `A.agent`
   - `composite_score` (u64) decimal-equals `A.composite_score`
   - `verified_at` (i64 unix seconds) equals the timestamp in
     `A.verified_at` (verifier MAY tolerate ±1 second clock-conversion
     drift)
   - `verification_hash` (32 bytes) hex-equals `A.verification_hash`
   - `content_hash`, `content_id_hash` likewise
   - `state` (u8) maps to `A.state` per the protocol's published enum
   Reject on any mismatch with `field_mismatch:<field>`.

6. **Domain bound.** `composite_score` ≤ `score_max`. Reject otherwise
   — protocols MUST emit attestations only for in-bounds scores.

7. **State validity.** `A.state` MUST be one of the states the protocol
   publishes as "valid for attestation". For Shillbot v1 this is
   exactly `"verified"`. Reject any other state.

A verifier that completes steps 1–7 MAY return a verdict object:

```json
{
  "valid": true,
  "attestation": { ...echo... },
  "checked_at": "2026-05-02T12:34:56Z",
  "rpc_endpoint": "https://api.mainnet-beta.solana.com"
}
```

If any step fails the verdict object MUST include a `failure_reason`
field (one of the strings in steps 2–6, with field name on
`field_mismatch:`).

---

## 5. Trust model

A successful verification of attestation `A` proves:

- `A.client` (Solana wallet) chose to escrow work to `A.agent` (another
  Solana wallet) under the protocol at `A.program_id` on `network`.
- The protocol's verification logic ran and recorded `A.composite_score`
  on-chain at `A.verified_at`.
- The on-chain account is still open at `checked_at` (see §6).

What it does NOT prove:

- That `A.composite_score` is "good" or "deserved." Score semantics are
  protocol-specific; AAS only exposes the binding.
- That `A.agent` is the same human/entity that controlled the wallet at
  task-claim time (wallet ownership can transfer).
- That the protocol's verification was correct (an oracle bug would
  produce on-chain scores that AAS happily attests to).

The verifier inherits the protocol's trust assumptions. A verifier that
only accepts attestations from `program_id == <hash>` is implicitly
trusting that protocol's deployer/upgrade authority. AAS does not
solve protocol-level trust; it solves portability.

---

## 6. Capture window (Shillbot-specific, normative for v1 emitters
   built on the same close-on-finalize pattern)

Shillbot's `finalize_task` instruction transitions the on-chain
state Verified → Finalized AND closes the account in the same call.
Once finalized, `getAccount` returns `null` for the PDA — the
attestation becomes permanently unverifiable.

**Implication for emitters:** the `GET /tasks/:id/attestation`
endpoint is only meaningful between `verify_task` landing and
`finalize_task` landing (the challenge window). After finalize, the
endpoint MUST return 409 with a clear "account closed" message.

**Implication for consumers:** capture the JSON during the challenge
window. Once captured, the JSON is portable, but verifiers will
reject it (step 2 of §4) once the account closes.

**Future work for v2:** an "archived attestation" pathway that stores
the verified tuple in an immutable log (e.g. a separate on-chain
account that doesn't close, or a Switchboard-attested snapshot). Out
of scope for v1.

---

## 7. Conformance

A protocol claims AAS v1 conformance by:

1. Emitting attestations matching §3.
2. Documenting which on-chain states are "valid for attestation" (must
   be a subset of the protocol's state machine).
3. Pointing to (or shipping) a verifier that implements §4 against
   real network reads.

A verifier claims AAS v1 conformance by:

1. Implementing all of §4 against the protocol's named program.
2. Treating unknown fields inside `extensions` as opaque (preserve, do
   not fail).
3. Returning structured verdicts with the failure-reason taxonomy in §4.

The reference verifiers in `services/aas-verifier-ts` (TypeScript) and
`services/aas-verifier-py` (Python) are normative for resolving
ambiguity in §4 — if the spec text and the reference disagree, the
reference is authoritative until v2 lands.

---

## 8. Differences from AAS v0

v0 was an unstable transitional format shipped under
`version: "aas/v0"`. v1 fixes:

- **`task_id` and `composite_score` are strings, not JSON numbers.**
  v0 shipped them as numbers; the JS-precision concern raised in
  critique-16 finding #4 motivates the change. v1 verifiers MUST
  parse strings; v0 emitters MUST migrate before v2.
- **No hash recomputation recipe.** v0's `verifier_instructions`
  claimed a recipe that didn't match the on-chain hash. v1 says
  verifiers re-read on-chain; the hash recomputation utility is
  removed. (This was critique-16 finding #1.)
- **`account` + `account_kind` instead of `task_pda`.** v0 named the
  Shillbot-specific field. v1 generalizes to any Anchor account.
- **`oracle_feed` is optional and explicit-null when absent.** v0
  required `switchboard_feed` even for protocols that don't use
  Switchboard. v1 lets non-Switchboard protocols emit `null`.
- **Extension envelope.** v1 adds `extensions: object` so protocols
  can add fields without breaking conformance.

---

## 9. Open questions for AAS v2

- **Cross-chain attestations.** Same shape, different `network` /
  `program_id` discriminator? Or a separate `chain_kind` field?
  The reference verifiers landing in #18 will surface implementation
  pressure that informs the v2 decision.
- **Aggregate proofs.** "Agent X has 50 verified attestations averaging
  0.82" is a higher-level construct that v1 doesn't address.
  Composable from v1 attestations off-chain; on-chain merkle-tree
  rollups are a v2 candidate.
- **Revocation.** Today the on-chain account closing IS the revocation
  signal. A protocol that wants to invalidate a verified attestation
  without closing the account would need v2.
- **Finalized-task attestations.** The capture-window gotcha (§6) is a
  real UX problem for consumers that want to attach a portable proof
  to a long-finished task. v2 candidate: cache the attestation tuple
  in a separate on-chain account at finalize time, or in a Switchboard
  snapshot, so the proof survives.

---

## 10. References

- AAS v0 emitter (Shillbot): `coordination-app/backend/shillbot-orchestrator/src/services/task_service.rs::build_attestation` + `routes/tasks.rs::get_attestation`
- AAS v0 MCP tool: `swarm-tips-repo/services/mcp-server/src/server.rs::shillbot_get_attestation`
- Shillbot on-chain `verify_task`: `swarm-tips-repo/programs/shillbot/src/instructions/verify_task.rs`
- Shillbot on-chain `Task` account schema: `swarm-tips-repo/programs/shillbot/src/state/task.rs`
- Switchboard feed: compile-time-locked at `swarm-tips-repo/programs/shillbot/src/constants.rs::SWITCHBOARD_FEED`
- Reference verifiers (planned): `swarm-tips-repo/services/aas-verifier-ts` and `services/aas-verifier-py` (task #18)
