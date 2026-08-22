# Shillbot — On-Chain Program Spec

Solana program (Anchor) for the Shillbot task marketplace. For product context and protocol overview, see `swarm/shillbot/CLAUDE.md`. For shared code standards, see the root `CLAUDE.md`. This file covers implementation-specific details only.

---

## Overview

Manages the full task lifecycle: task creation with escrow, agent claiming, proof submission, oracle-verified scoring, optimistic finalization with challenge window, and performance-scaled payment release.

Uses `init` exclusively for all accounts except `AgentState` and `ClientState`, which use `init_if_needed` (the signing agent/client pays for their own PDA, no escrow funds, idempotent). `ClientState` has no close instruction, so its rate-limit counters cannot be reset by close-and-recreate.

---

## State Machine

```
         ──(create_task)──► Open
Open ──(claim_task)──► Claimed
Open ──(expire_task: past deadline)──► [escrow returned, account closed]
Open ──(emergency_return)──► [escrow returned, account closed]
Claimed ──(submit_work)──► Submitted
Claimed ──(expire_task: past deadline)──► [escrow returned, account closed]
Claimed ──(emergency_return)──► [escrow returned, account closed]
Submitted ──(approve_task: client signs)──► Approved
Submitted ──(expire_task: T+14d verification timeout from submitted_at)──► [escrow returned, account closed]
Approved/Submitted ──(verify_task: Switchboard oracle, verification_kind=0)──► Verified
Approved/Submitted ──(verify_task_attested: oracle_authority signer, verification_kind=1)──► Verified
Approved ──(expire_task: T+14d verification timeout from submitted_at)──► [escrow returned, account closed]
Verified ──(finalize_task: challenge window passes)──► Finalized → [payment released, account closed]
Verified ──(challenge_task)──► Disputed [sets dispute-resolution deadline when window enabled]
Disputed ──(resolve_challenge: authority adjudicates)──► Resolved → [payments adjusted, account closed]
Disputed ──(resolve_challenge_default: permissionless, past challenge.created_at
            + dispute_resolution_window_seconds)──► Resolved → [pinned payment executes,
            bond returned un-slashed, account closed]
```

Every instruction asserts valid source state(s) as a precondition. Invalid state transitions return `InvalidTaskState`.

**Verification kinds (`Task.verification_kind`, carved from `_reserved` 2026-07-07):**
`0 = OracleMetrics` (legacy Switchboard path — `verify_task` only), `1 = DeterministicAttested`
(allow-listed attester path — `verify_task_attested` only; score must be exactly 0 or
MAX_SCORE). The two verify entries are mutually exclusive on this byte. Wire-format twin:
`chain-core::verify_schema::VerificationKind` (append-only). `verify_task_attested` requires
the `oracle_authority` as a transaction `Signer` — the first instruction to enforce that
(previously dormant) GlobalState field — and stores the `AttestationCert` digest as
`verification_hash`. Arms-length guard: the attester must not equal `task.agent`.
(The former kind-1 self-claim guard — `agent != client` on `claim_task` /
`claim_task_session` — was removed: it only blocked same-wallet credential wash,
which a two-wallet poster/worker split bypasses, so it protected nothing real
while adding friction and a kind-0/kind-1 inconsistency. A Lean credential is
worth what its poser is worth; that weighting is off-chain. `SelfClaimForbidden`
is retained unused in the error enum to preserve Anchor numbering.)

**Dispute-resolution liveness (2026-07-07):** `challenge_task` previously moved a task into
Disputed with NO bound on how long the single authority could sit on `resolve_challenge`
(escrow + bond frozen forever if the authority disappears). `GlobalState.
dispute_resolution_window_seconds` (carved from `_reserved`; 0 = disabled → legacy
behavior) now arms a permissionless `resolve_challenge_default` once
`now > challenge.created_at + window`: the pinned payment/fee execute (the task WAS
verified — agent-favoring by design), the bond returns to the challenger un-slashed (no
adjudication happened), and the task closes as Resolved. Challenges are a bounded delay,
never a freeze or a grief profit.

---

## Accounts

| Account | PDA Seeds | Purpose |
|---|---|---|
| `GlobalState` | `["shillbot_global"]` | Singleton config: task counter, authority, treasury, fee/threshold params |
| `Task` | `["task", task_counter (8-byte LE), client]` | Task data + escrow lamport vault |
| `Challenge` | `["challenge", task_id (8-byte LE), challenger]` | Challenge bond + resolution state |
| `AgentState` | `["agent_state", agent_pubkey]` | Tracks `claimed_count`, `total_completed`, `total_earned`, `total_score_sum`, `total_tasks_claimed`, `total_challenges_lost` (Phase 1 reputation counters; new fields in #12 carved out of `_reserved`, no realloc) |
| `SessionDelegate` | `["session", agent, delegate]` | Scoped session key delegation (bitmask: 0x01=claim, 0x02=submit) |
| `Identity` | `["identity", agent, &[platform]]` | Maps agent wallet to platform identity hash |
| `ClientState` | `["client_state", client_pubkey]` | Per-client task-creation rate limit (sliding 1h window) + lifetime task counter (Phase 3 blocker #2) |

See `state/*.rs` for full field layouts.

---

## Instructions

### Core Task Lifecycle
- `initialize(protocol_fee_bps, quality_threshold, starting_counter)` — one-time GlobalState setup
- `create_task(escrow_lamports, content_hash, deadline, submit_margin, claim_buffer, platform, attestation_delay_override, challenge_window_override, verification_timeout_override)` — client creates task, funds escrow, generates task_nonce from slothash
- `claim_task()` — agent claims Open task; enforces claim_buffer and max concurrent claims via AgentState
- `submit_work(content_id)` — agent submits content ID proof; enforces submit_margin
- `approve_task()` — client signs to approve submitted content; transitions Submitted → Approved (Phase 3 blocker #3a). The verification timeout clock is NOT reset by approval — it remains anchored on `submitted_at`, so a client cannot indefinitely stall an agent's escrow by approving and then never funding oracle verification.
- `verify_task(composite_score, verification_hash)` — Switchboard oracle attestation on a `verification_kind = 0` task; computes payment, sets challenge window
- `verify_task_attested(score, verification_hash)` — attester-signed verification on a `verification_kind = 1` task; the `oracle_authority` signs the transaction, `score ∈ {0, MAX_SCORE}`, `verification_hash` = the `chain-core::verify_schema::AttestationCert` digest
- `finalize_task()` — permissionless crank after challenge window; releases payment to agent, fee to treasury, remainder to client
- `challenge_task()` — anyone posts bond (2-10x escrow) to dispute during challenge window
- `resolve_challenge(challenger_won)` — authority resolves dispute; slashes loser's funds
- `resolve_challenge_default()` — permissionless crank once `dispute_resolution_window_seconds` (when enabled) elapses on a Disputed task; executes the pinned payment/fee, returns the bond un-slashed
- `expire_task()` — permissionless crank; returns escrow for expired Open/Claimed tasks or Submitted/Approved tasks past verification timeout (measured from `submitted_at`)
- `emergency_return()` — authority-only batch return of Open/Claimed task escrows (up to 20 tasks)

### Session Delegation
- `create_session(allowed_instructions, duration_seconds)` — agent creates scoped session key
- `revoke_session()` — agent revokes session key
- `claim_task_session()` — session-delegated claim_task (bitmask 0x01)
- `submit_work_session(content_id)` — session-delegated submit_work (bitmask 0x02)

### Identity
- `register_identity(platform, identity_hash)` — agent maps wallet to platform identity
- `revoke_identity()` — agent removes identity mapping

### Admin
- `update_params(...)` — authority updates protocol params (fee, threshold, windows, multipliers, pause state)
- `transfer_authority(new_authority)` — transfer GlobalState authority
- `update_treasury(new_treasury)` — change treasury address
- `update_oracle_authority(new_oracle_authority)` — change oracle signer
- `close_agent_state()` — close AgentState PDA, return rent
- `migrate_agent_state()` — one-time PDA-size migration (42 → 90 bytes) for v1 `AgentState` accounts predating the v2 layout extension; preserves `claimed_count` + `bump`, zero-inits the v2 counters

**Switchboard feed:** stored as a mutable field on `GlobalState.switchboard_feed`, set by `initialize` and read by `verify_task` (`verify_task.rs:90-97`). An earlier iteration compile-time-locked the feed in `constants.rs::SWITCHBOARD_FEED` to remove the single-key compromise path, but the lock was reverted 2026-05-08 because it foreclosed feed rotation in response to oracle outages. The `set_switchboard_feed` instruction is **not** currently restored — rotation today requires a program upgrade that re-runs `initialize`. Restoring `set_switchboard_feed` (with rate-limiting + telemetry) is a deferred follow-up. **USER MUST PASS** the production Switchboard pull-feed pubkey to `initialize` at deploy time — without it, mainnet `verify_task` calls fail closed (caller's feed account won't match the on-chain pubkey).

---

## Payment Computation

```
if composite_score < quality_threshold:
    payment = 0, fee = 0
else:
    score_range = MAX_SCORE - quality_threshold
    score_above = composite_score - quality_threshold
    gross_payment = escrow * score_above / score_range
    fee = gross_payment * protocol_fee_bps / 10_000
    payment = gross_payment - fee
```

All arithmetic uses `checked_*` with u128 intermediates. Postcondition: `payment + fee <= escrow_lamports`.

Challenge bond: `multiplier * escrow_lamports` where multiplier is in [2, 10].

---

## Immutable Invariants

1. Escrow release requires valid oracle attestation
2. Payment requires `composite_score >= quality_threshold`
3. Challenge window must exist before finalization
4. Verification timeout at T+14d returns escrow if no attestation
5. Strict state machine enforcement — every instruction asserts valid source states
6. CEI ordering — all state mutations before any CPI
7. `payment + fee <= escrow_lamports` asserted before every transfer

---

## Parameter Governance

Authority can modify via `update_params`:
- `protocol_fee_bps` — bounds [100, 2500] (1-25%)
- `quality_threshold` — bounded by authority
- `challenge_window_seconds`, `verification_timeout_seconds`, `attestation_delay_seconds`, `staleness_window_seconds`
- `max_concurrent_claims`, `challenge_bond_multiplier`, `bond_slash_treasury_bps`
- `paused`, `paused_platforms` — emergency pause controls

---

## Known limitations

- **Approval-grief vector (Phase 3 blocker #3a residual).** A malicious client can create a task, wait for an agent to submit work, and then never call `approve_task`. The agent's escrow stays locked until `expire_task` returns it at T+verification_timeout (~14 days default). The agent's `claimed_count` is decremented on `submit_work`, so the agent can claim other tasks during this period — but the specific escrow is dead capital. The per-client rate limit (Phase 3 blocker #2: 10 tasks/hour) caps the harm at 10 agents per malicious client per hour. A first-class `reject_task` instruction with reason capture (Phase 3 blocker #3a follow-up) would let agents re-claim their attention faster than the timeout. Future hardening: client reputation slashing on excessive non-approval rates, or a shorter timeout when the client is silent vs. actively rejecting.

- **AgentState as optional remaining_account (#12 inheritance).** `finalize_task` and `resolve_challenge` accept the agent's `AgentState` PDA as `remaining_accounts[0]`; if the caller omits it, the agent's reputation counters (`total_completed`, `total_earned`, `total_score_sum`, `total_challenges_lost`) silently don't update. `claim_task` is unaffected — it requires AgentState as a named account, so `total_tasks_claimed` always updates. Risk: an agent with N finalized tasks where only M < N calls passed AgentState reports `total_completed = M`, `total_score_sum = (sum of M scores)` — `average_score` is correct over the counted subset, but `completion_rate = total_completed / total_tasks_claimed` is artificially low (denominator bumped by every claim, numerator bumped only when the caller passed AgentState). Currently mitigated operationally — the orchestrator and MCP server always pass AgentState — but on-chain enforcement (making AgentState a required account on those instructions) would be more robust. Tracked as a Phase 1 reputation hardening follow-up; not required for the agent_profile MCP tool's v1 launch because all production callers pass the account.
