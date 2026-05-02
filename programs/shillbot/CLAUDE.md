# Shillbot — On-Chain Program Spec

Solana program (Anchor) for the Shillbot task marketplace. For product context and protocol overview, see `swarm/shillbot/CLAUDE.md`. For shared code standards, see the root `CLAUDE.md`. This file covers implementation-specific details only.

---

## Overview

Manages the full task lifecycle: task creation with escrow, agent claiming, proof submission, oracle-verified scoring, optimistic finalization with challenge window, and performance-scaled payment release.

Uses `init` exclusively for all accounts except `AgentState`, which uses `init_if_needed` (agent pays, no escrow funds, idempotent across first claim).

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
Approved ──(verify_task: oracle attestation)──► Verified
Approved ──(expire_task: T+14d verification timeout from submitted_at)──► [escrow returned, account closed]
Verified ──(finalize_task: challenge window passes)──► Finalized → [payment released, account closed]
Verified ──(challenge_task)──► Disputed
Disputed ──(resolve_challenge)──► Resolved → [payments adjusted, account closed]
```

Every instruction asserts valid source state(s) as a precondition. Invalid state transitions return `InvalidTaskState`.

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
- `verify_task(composite_score, verification_hash)` — oracle attestation on an Approved task; computes payment, sets challenge window
- `finalize_task()` — permissionless crank after challenge window; releases payment to agent, fee to treasury, remainder to client
- `challenge_task()` — anyone posts bond (2-10x escrow) to dispute during challenge window
- `resolve_challenge(challenger_won)` — authority resolves dispute; slashes loser's funds
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

**Removed (Phase 3 blocker #1 Path A, ~2026-05-01):** `set_switchboard_feed` was authority-mutable, which created a single-key compromise path to attacker-controlled scores. The Switchboard feed pubkey is now compile-time-locked in `programs/shillbot/src/constants.rs::SWITCHBOARD_FEED` and read directly by `verify_task` — the on-chain `GlobalState.switchboard_feed` field is vestigial (kept for schema compat; not consulted by any instruction). Future feed changes require a program upgrade signed by the upgrade authority (Squads multisig on mainnet). The `SwitchboardFeedUpdated` event was removed alongside the instruction. **USER MUST FILL** the const in `programs/shillbot/src/constants.rs` with the production Switchboard pull-feed pubkey before any mainnet program upgrade — without the swap, mainnet `verify_task` calls fail closed (caller's feed account won't match the placeholder pubkey).

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

Authority (Squads multisig on mainnet) can modify via `update_params`:
- `protocol_fee_bps` — bounds [100, 2500] (1-25%)
- `quality_threshold` — bounded by authority
- `challenge_window_seconds`, `verification_timeout_seconds`, `attestation_delay_seconds`, `staleness_window_seconds`
- `max_concurrent_claims`, `challenge_bond_multiplier`, `bond_slash_treasury_bps`
- `paused`, `paused_platforms` — emergency pause controls

---

## Known limitations

- **Approval-grief vector (Phase 3 blocker #3a residual).** A malicious client can create a task, wait for an agent to submit work, and then never call `approve_task`. The agent's escrow stays locked until `expire_task` returns it at T+verification_timeout (~14 days default). The agent's `claimed_count` is decremented on `submit_work`, so the agent can claim other tasks during this period — but the specific escrow is dead capital. The per-client rate limit (Phase 3 blocker #2: 10 tasks/hour) caps the harm at 10 agents per malicious client per hour. A first-class `reject_task` instruction with reason capture (Phase 3 blocker #3a follow-up) would let agents re-claim their attention faster than the timeout. Future hardening: client reputation slashing on excessive non-approval rates, or a shorter timeout when the client is silent vs. actively rejecting.
