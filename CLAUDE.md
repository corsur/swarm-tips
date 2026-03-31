# Smart Contracts — Implementation Context

Solana programs built with Anchor. Read the root `coordination/CLAUDE.md` before this file for project context, code standards, and DAO overview. Read `shillbot/CLAUDE.md` for the Shillbot protocol specification.

This workspace contains all on-chain programs for the Coordination DAO: the coordination game, the Shillbot task marketplace, and shared types.

---

## Code Standards (Self-Contained for Solana Programs)

These are the full code standards for this workspace, translated from the root `coordination/CLAUDE.md` into Anchor/Solana-specific rules. An agent working only in this directory gets everything it needs from this section.

### Design Philosophy

Write clean, minimal code. Complexity is a liability — every line of code is a line that can be wrong.

- **No speculative abstraction.** Don't create a generic `GameEngine` trait because "we might have more game types." Build the coordination game. Build the shillbot task lifecycle. If a pattern emerges across both, extract it then. The `shared` crate has the platform-agnostic types (`PlatformProof`, `EngagementMetrics`) — those are justified because changing on-chain types requires a program upgrade. Everything else starts concrete.
- **Delete, don't comment out.** No `// old payoff logic` or `// TODO: remove`. Git has the history. Commented-out instruction handlers are noise.
- **Names over comments.** `fn verify_commitment_matches_preimage(commitment: &[u8; 32], preimage: &[u8; 32]) -> bool` needs no comment. `fn check(c: &[u8; 32], r: &[u8; 32]) -> bool` needs a rewrite.
- **Flat over nested.** Use early returns with `require!()`: check all preconditions at the top of the handler, then the happy path is flat. If a handler has 3 levels of `if` nesting, break it up.
- **No clever code.** A `match` on `TaskState` is better than casting to `u8` and doing arithmetic on state values. The next auditor shouldn't have to decode your intent.
- **Refactor as you go.** When touching a file, fix naming, remove dead code, simplify structure. Leave every file cleaner than you found it.
- **Don't write tests until you know what behavior you want.** The spec (this file) defines the behavior. Tests encode the spec. A test that doesn't trace back to a spec requirement is worthless.
- **No worthless tests.** `assert!(create_task_works())` is not a test. A test that creates a task with specific parameters, then verifies the PDA state, escrow balance, and emitted event is a test.
- **No error swallowing.** Every failure mode must be visible. Every `require!()` uses a named error variant. Every error variant is documented. No `ProgramError::Custom(0)`.
- **Reject at system boundaries.** Every instruction handler validates ALL inputs before touching state. A game that accepts an invalid stake amount is worse than one that refuses a valid one.
- **Diagrams are mandatory.** Both state machines (game and shillbot) have ASCII diagrams in this file. When modifying state transitions, update the diagram FIRST. If a new instruction adds a transition not in the diagram, the change is incomplete.
- **Everything deferred is written down.** Open questions and deferred work live in the Open Questions section at the bottom of this file. Not as code comments.
- **Classify by reversibility.** On-chain decisions are almost all one-way doors: account structures, PDA seed derivation, state machine transitions, payment formulas. These require maximum rigor. The only two-way doors: parameter values (quality threshold, protocol fee, scoring weights) which are governance-adjustable within bounds. Treat everything else as irreversible.

### Rules — Anchor/Solana Specific

**Rule 1 — No recursion.** Solana BPF has a 4KB call stack. Recursive functions are banned. All iteration must be explicit and bounded. If processing remaining accounts in `finalize_tournament`, iterate with a `for` loop and a counter, never recurse.

**Rule 2 — Bounded loops.** Every loop iterating over instruction input must check its bound BEFORE entry:
```rust
require!(remaining_accounts.len() <= MAX_PLAYERS, TooManyAccounts);
for account_info in ctx.remaining_accounts.iter() {
    // safe: bounded by the check above
}
```
Never trust `remaining_accounts.len()` — an attacker chooses how many accounts to pass.

**Rule 3 — No unbounded resource consumption.** On-chain, collection sizes must be statically bounded. Never use `Vec::with_capacity(n)` where `n` comes from instruction input. Every PDA account has a fixed, known size. If a collection grows with usage (e.g., challenge history), use separate PDA accounts per entry, not a growing Vec in one account.

**Rule 4 — Instruction handlers ≤100 lines.** Handlers validate, delegate to a pure function, and emit an event:
```rust
pub fn reveal_guess(ctx: Context<RevealGuess>, r: [u8; 32]) -> Result<()> {
    let game = &mut ctx.accounts.game;
    // Checks (10 lines)
    require!(game.state == GameState::Revealing, InvalidGameState);
    require!(!already_revealed(game, player), AlreadyRevealed);
    verify_commitment(game, player, &r)?;
    // Effects (5 lines)
    let guess = r[31] & 1;
    set_guess(game, player, guess);
    // Interactions (5 lines) — if both revealed, resolve
    if both_revealed(game) {
        resolve_game(game, &ctx.accounts.tournament, ...)?;
    }
    emit!(GuessRevealed { game_id: game.game_id, player: player.key() });
    Ok(())
}
```
`resolve_game` is a pure function that computes payoffs. The handler orchestrates but doesn't compute.

**Rule 5 — Assert invariants (minimum 2 per function).** For every instruction handler:
```rust
// Precondition: game is in the correct state
require!(game.state == GameState::Verified, InvalidTaskState);
// Precondition: challenge window has passed
require!(clock.unix_timestamp > task.challenge_deadline, ChallengeWindowOpen);
// ... do the work ...
// Postcondition: lamport conservation
let total_out = payment + fee + remainder;
require!(total_out == task.escrow_lamports, PaymentExceedsEscrow);
```
Pure functions called by handlers also assert: preconditions on entry, postconditions on exit.

**Rule 6 — Smallest data scope.** In Anchor, only request the accounts and permissions an instruction actually needs. If `finalize_task` doesn't need the client's wallet as a signer, don't include it in the Accounts struct. Excess authority is excess attack surface. Don't make accounts `mut` if the instruction only reads them.

**Rule 7 — No .unwrap() or .expect().** Every fallible call uses `?` or an explicit match. A panic in a Solana program aborts the transaction and can leave state inconsistent (if a CPI succeeded before the panic). Use `ok_or(ErrorCode::ArithmeticOverflow)?` for Option types. Use `checked_add`, `checked_mul`, `checked_div`, `checked_sub` — never raw operators on values that could overflow.

**Rule 8 — No unsafe.** Zero `unsafe` in smart contract code. If you think you need it, you're solving the wrong problem. Anchor and the Solana SDK provide safe abstractions for everything.

**Rule 9 — Warnings as errors.** Every program crate:
```rust
#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]
```
`clippy::arithmetic_side_effects` means the compiler rejects `+`, `-`, `*`, `/` on integers — you MUST use `checked_*` variants. This is non-negotiable for code handling lamports, scores, and fees.

**Rule 10 — Release mode for production.** BPF builds are always release mode. Debug overflow checks don't exist in production. Every arithmetic safety check must be explicit (`checked_*`), never reliant on debug assertions. This is the #1 source of Solana exploits — developers test with debug overflow detection, deploy to mainnet where it's gone, and get drained.

**Rule 11 — CEI (Checks-Effects-Interactions).** Instruction handlers follow this order exactly:
1. **Checks** — all `require!()` validations. State checks, signer checks, bounds checks, timeout checks.
2. **Effects** — all `account.field = value` mutations. State transitions, score recording, timestamp updates.
3. **Interactions** — all lamport transfers and CPIs. `system_program::transfer`, `token::transfer`, emit events.
Never transfer lamports before state is committed. Never CPI before all account mutations are done. In Anchor, this means every `ctx.accounts.game.state = GameState::Resolved` comes BEFORE every `transfer_lamports(...)`.

### Solana Security Rules

- **Account ownership** — always verify an account is owned by the expected program before reading its data. Anchor typed accounts enforce this automatically. Never bypass with raw `AccountInfo` unless ownership is manually verified with `require!(account.owner == &expected_program_id)`.
- **Signer checks** — never assume an account signed. Always verify via Anchor's `Signer` type or explicit `require!(account.is_signer)`.
- **PDA derivation** — always re-derive and verify PDA seeds on-chain. Never trust a PDA address passed in by a caller. Use Anchor's `seeds` and `bump` constraints.
- **State before CPI** — finalize ALL account state mutations before ANY cross-program invocation. A CPI to an untrusted program after partial state updates is a reentrancy risk.

### Fixed-Point Arithmetic

All scoring and payment calculations use integer arithmetic with an explicit scaling factor (basis points with 10,000 denominator, or 1e6 for composite scores). Document the precision guarantees. Use u128 for intermediate products to prevent overflow on large metric values. Assert `payment_amount + fee <= escrow_lamports` before any transfer.

### `init` vs `init_if_needed`

Use `init` exclusively for the shillbot program (prevents PDA account resurrection attacks), with one exception: `AgentState` uses `init_if_needed` because (a) the agent pays for creation, (b) it holds no escrow funds, and (c) it must be idempotent across the agent's first claim. The coordination game uses `init_if_needed` for PlayerProfile accounts only (player pays for creation, idempotent). Never use `init_if_needed` for accounts holding escrow funds.

### Crate-Level Requirements

Every program crate in this workspace must include:
```rust
#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]  // mandatory for all financial arithmetic
```
BPF builds are release mode — debug overflow checks do not apply in production. All arithmetic safety must be explicit (`checked_add`, `checked_mul`, etc.), never reliant on debug assertions. This is Rule 10 from the root CLAUDE.md and is the #1 source of Solana exploits.

### Observability (On-Chain Adaptation)

On-chain programs cannot log to Cloud Logging. The observability layer is:
- **Events** — emit an Anchor event for every state transition, every payment, every slash, every challenge. These are the on-chain equivalent of structured logs. Off-chain indexers consume them for monitoring, alerting, and dashboards.
- **`msg!()`** — use for debugging during development. Strip or minimize in production builds (consumes compute units).
- **Named error variants** — every `require!` / error return uses a specific error from the program's error enum. Generic errors (`ProgramError::Custom(0)`) are banned. The error name IS the structured log entry.

### Deployment

`anchor deploy` from a local machine is forbidden on mainnet. All mainnet deployments go through CI with the upgrade authority check (assert Squads multisig, fail if EOA). Devnet deployments from local machines are acceptable during development.

### Root Philosophy — Solana Adaptations

These apply the root CLAUDE.md philosophy items to the smart contract context:

**"Every failure mode must be visible"** — On-chain, this means: every `require!` / error return must use a specific named error variant (not a generic error). Every error variant must be documented with what triggers it. On-chain programs cannot log to Cloud Logging, so **events are the observability layer** — emit events for every state transition, every payment, every slash. Off-chain indexers consume these events for monitoring and alerting.

**"Diagrams are mandatory for non-trivial flows"** — Both state machines in this workspace (game and shillbot) have ASCII diagrams in this file. When modifying state transitions, update the diagram first, then the code. If a new instruction adds a state transition not in the diagram, the PR is incomplete. Code comments in `state/*.rs` files should include inline ASCII diagrams for any non-obvious account relationship or data flow.

**"Everything deferred must be written down"** — Deferred work for smart contracts is tracked in the Open Questions section at the bottom of this file. If an instruction handler has a known limitation or a future enhancement is planned, add it there — not as a code comment that gets forgotten.

**"Classify decisions by reversibility"** — Smart contract decisions are almost all one-way doors: account structures, PDA seed derivation, state machine transitions, and payment formulas are extremely hard to change post-deployment (requires program upgrade with timelock). These deserve maximum rigor. The only two-way doors in this workspace are: parameter values (quality threshold, protocol fee, scoring weights) which are governance-adjustable within bounds, and off-chain oracle configuration. Treat everything else as irreversible.

---

## Workspace Structure

```
smartcontracts/
├── Anchor.toml
├── Makefile                            # make build / test / clean
├── Cargo.toml                          # workspace root
├── programs/
│   ├── coordination/                   # The Coordination Game (existing)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── state/
│   │       │   ├── game.rs             # Game account, GameState enum
│   │       │   ├── player.rs           # PlayerProfile account
│   │       │   ├── tournament.rs       # Tournament account
│   │       │   └── escrow.rs
│   │       ├── instructions/
│   │       │   ├── initialize.rs       # one-time global setup (GameCounter)
│   │       │   ├── create_tournament.rs
│   │       │   ├── create_game.rs
│   │       │   ├── join_game.rs
│   │       │   ├── commit_guess.rs
│   │       │   ├── reveal_guess.rs
│   │       │   ├── resolve_timeout.rs
│   │       │   ├── close_game.rs
│   │       │   ├── finalize_tournament.rs
│   │       │   └── claim_reward.rs
│   │       ├── errors.rs
│   │       ├── events.rs
│   │       └── payoff.rs
│   │
│   ├── shillbot/                       # Shillbot Task Marketplace (NEW)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── state/
│   │       │   ├── task.rs             # Task account, TaskState enum
│   │       │   ├── global.rs           # GlobalState (monotonic task counter)
│   │       │   └── challenge.rs        # Challenge account
│   │       ├── instructions/
│   │       │   ├── initialize.rs       # one-time setup (GlobalState)
│   │       │   ├── create_task.rs      # client creates task, funds escrow
│   │       │   ├── claim_task.rs       # agent claims, deadline locked
│   │       │   ├── submit_work.rs      # agent submits video ID proof
│   │       │   ├── verify_task.rs      # oracle attestation records score
│   │       │   ├── finalize_task.rs    # challenge window passes, payment releases
│   │       │   ├── challenge_task.rs   # anyone posts bond to dispute
│   │       │   ├── resolve_challenge.rs # multisig resolves dispute
│   │       │   ├── expire_task.rs      # permissionless crank, returns escrow
│   │       │   ├── emergency_return.rs # multisig returns Open/Claimed escrow
│   │       │   ├── revoke_session.rs   # agent revokes MCP session delegation
│   │       │   ├── claim_task_session.rs  # session-delegated claim_task
│   │       │   └── submit_work_session.rs # session-delegated submit_work
│   │       ├── scoring.rs              # composite score computation (fixed-point)
│   │       ├── errors.rs
│   │       └── events.rs
│   │
│   └── shared/                         # Shared types library crate (NOT a program)
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── platform.rs             # PlatformProof, EngagementMetrics traits
│           ├── scoring.rs              # CompositeScore, ScoringWeights types
│           └── constants.rs            # shared constants (max weights, fee bounds, etc.)
│
├── tests/
│   ├── coordination.ts                 # coordination game end-to-end tests
│   └── shillbot.ts                     # shillbot end-to-end tests
│
└── sdk/                                # TypeScript SDK (published to GitHub Packages)
```

Instruction handlers must be thin — validate, delegate to a pure function, emit event. Business logic lives in pure functions, not handlers. Each instruction is its own file.

---

## Program: coordination (Existing Game)

See the detailed specification below. This program is unchanged from the current implementation.

### Dependencies

```toml
[dependencies]
anchor-lang = { version = "0.32.1", features = ["init-if-needed"] }
solana-sha256-hasher = "2"      # sol_sha256 syscall binding for commit verification
shared = { path = "../shared" }  # shared types
```

### DAO Treasury Integration

The coordination game's tournament prize pool flows to the Coordination DAO treasury (Squads-controlled account). The `finalize_tournament` and `claim_reward` instructions interact with the Squads treasury PDA rather than a standalone tournament balance.

Losing stake from games accumulates in the Tournament PDA during the tournament, then the Squads multisig governs distribution post-finalization.

---

## Program: shillbot (NEW — Task Marketplace)

### Overview

Manages the full task lifecycle for the Shillbot protocol: task creation with escrow, agent claiming, proof submission, oracle-verified scoring, optimistic finalization with challenge window, and performance-scaled payment release.

### Dependencies

```toml
[dependencies]
anchor-lang = "0.32.1"
switchboard-on-demand = "..."   # Switchboard oracle integration
shared = { path = "../shared" } # shared platform-agnostic types
```

Note: uses `init` exclusively for all accounts except `AgentState`, which uses `init_if_needed` (agent pays, no escrow funds, idempotent across first claim).

### Account Structures

#### `GlobalState`

Singleton PDA. Seeds: `["shillbot_global"]`

```
discriminator:        8 bytes
task_counter:         u64       monotonic counter, incremented on each create_task
authority:            Pubkey    Squads multisig (mainnet) or EOA (devnet)
treasury:             Pubkey    treasury account for protocol fee collection
protocol_fee_bps:     u16       protocol fee in basis points (100 = 1%)
quality_threshold:    u64       minimum composite score for payment (fixed-point)
bump:                 u8
```

#### `Task`

PDA seeds: `["task", task_counter: u64 as 8-byte LE, client: Pubkey]`

```
task_id:              u64
client:               Pubkey
agent:                Pubkey      zero-key until claimed
state:                u8          TaskState enum
escrow_lamports:      u64         client's escrowed payment
content_hash:         [u8; 32]    SHA-256 of the off-chain campaign brief
video_id_hash:        [u8; 32]    SHA-256 of submitted YouTube video ID (zeroed until submitted)
task_nonce:           [u8; 16]    random nonce agent must include in video description
composite_score:      u64         fixed-point score from oracle attestation (0 until verified)
payment_amount:       u64         computed payment (0 until verified)
fee_amount:           u64         computed protocol fee (0 until verified, stored at verify time to prevent parameter-change bricking)
deadline:             i64         Unix timestamp
submit_margin:        i64         seconds before deadline that submission must occur
claim_buffer:         i64         minimum seconds remaining to claim
created_at:           i64
submitted_at:         i64         0 until submitted
verified_at:          i64         0 until oracle attestation
challenge_deadline:   i64         0 until challenge window starts
bump:                 u8
```

#### `Challenge`

PDA seeds: `["challenge", task_id: u64 as 8-byte LE, challenger: Pubkey]`

```
task_id:              u64
challenger:           Pubkey
bond_lamports:        u64
is_client_challenge:  bool        true if challenger == task.client (free if within 20% cap)
created_at:           i64
resolved:             bool
challenger_won:       bool
bump:                 u8
```

#### `SessionDelegate`

PDA seeds: `["session", agent: Pubkey, delegate: Pubkey]`

```
agent:                Pubkey      the agent who delegated
delegate:             Pubkey      the session key (MCP server holds this)
allowed_instructions: u8          bitmask: 0x01 = claim_task, 0x02 = submit_work
created_at:           i64
bump:                 u8
```

### State Machine

```
         ──(create_task)──► Open
Open ──(claim_task)──► Claimed
Open ──(expire_task: past deadline)──► [escrow returned, account closed]
Open ──(emergency_return)──► [escrow returned, account closed]
Claimed ──(submit_work)──► Submitted
Claimed ──(expire_task: past deadline)──► [escrow returned, account closed]
Claimed ──(emergency_return)──► [escrow returned, account closed]
Submitted ──(verify_task: oracle attestation)──► Verified
Submitted ──(expire_task: T+14d verification timeout)──► [escrow returned, account closed]
Verified ──(finalize_task: challenge window passes)──► Finalized → [payment released, account closed]
Verified ──(challenge_task)──► Disputed
Disputed ──(resolve_challenge)──► Resolved → [payments adjusted, account closed]
```

Every instruction asserts valid source state(s) as a precondition. Invalid state transitions return `InvalidTaskState`.

### Instructions

#### `initialize`
Signers: `authority`
- Creates the `GlobalState` singleton PDA
- Sets initial `protocol_fee_bps`, `quality_threshold`
- On devnet: authority is an EOA. On mainnet: authority is the Squads multisig.

#### `create_task`
Signers: `client`
- Increment `GlobalState.task_counter` (atomically with PDA init)
- Init Task PDA with `init` (NOT `init_if_needed`)
- Generate random `task_nonce` (16 bytes from recent slothash)
- Transfer `escrow_lamports` from client to Task PDA
- Store `content_hash` of the off-chain brief
- Set `state = Open`, `deadline`, `submit_margin`, `claim_buffer`
- Emit `TaskCreated { task_id, client, escrow_lamports, deadline, task_nonce }`

#### `claim_task`
Signers: `agent` (or authorized `SessionDelegate` with claim permission)
- Assert `state == Open`
- Assert `Clock::now() + claim_buffer < deadline` (minimum time buffer)
- Assert agent has fewer than 5 claimed-but-not-submitted tasks (concurrent claim limit)
- Set `agent`, `state = Claimed`
- Emit `TaskClaimed { task_id, agent }`

Concurrent claim check: enforced via `AgentState` PDA (seeds: `["agent_state", agent_pubkey]`), which tracks `claimed_count` on-chain. Incremented by `claim_task`, decremented by `submit_work` and `expire_task`. Uses `init_if_needed` (agent pays, no escrow — see code comment for justification).

#### `submit_work`
Signers: `agent` (or authorized `SessionDelegate` with submit permission)
- Assert `state == Claimed`
- Assert `agent == task.agent`
- Assert `Clock::now() + submit_margin < deadline` (submission before deadline minus margin)
- Store `video_id_hash = SHA-256(video_id)`
- Set `submitted_at = Clock::now()`, `state = Submitted`
- Emit `WorkSubmitted { task_id, agent, video_id_hash }`

#### `verify_task`
Authority: Switchboard feed attestation (verified via Switchboard account ownership + feed PDA derivation from fixed seeds)
- Assert `state == Submitted`
- Assert attestation account is owned by Switchboard program
- Assert attestation feed PDA matches the expected feed derived from well-known fixed seeds (immutable — feed rotation requires program upgrade)
- Assert staleness: attestation timestamp is within acceptable window of `submitted_at + 7 days`
- Read composite score from attestation data
- Assert composite score is within valid bounds (0 to MAX_SCORE)
- **Circuit breaker:** if score is 0 or attestation data is missing, do NOT finalize — set a flag for manual review
- Store `composite_score`, set `verified_at`, compute `challenge_deadline = Clock::now() + CHALLENGE_WINDOW`
- Set `state = Verified`
- Compute `payment_amount`: if `composite_score >= quality_threshold`, scale linearly from threshold to max. If below threshold, `payment_amount = 0`.
- Emit `TaskVerified { task_id, composite_score, payment_amount }`

Payment computation (fixed-point):
```
if composite_score < quality_threshold:
    payment_amount = 0
else:
    // Linear scaling from threshold to max
    score_range = MAX_SCORE - quality_threshold
    score_above_threshold = composite_score - quality_threshold
    payment_amount = escrow_lamports * score_above_threshold / score_range
    // Deduct protocol fee
    fee = payment_amount * protocol_fee_bps / 10_000
    payment_amount = payment_amount - fee
```
All arithmetic uses `checked_*` operations. Intermediate products use u128. Assert `payment_amount + fee <= escrow_lamports`.

#### `finalize_task`
Permissionless crank — anyone can call after challenge deadline.
- Assert `state == Verified`
- Assert `Clock::now() > challenge_deadline`
- If `payment_amount > 0`: transfer `payment_amount` to agent, transfer `fee` to treasury, return remainder to client
- If `payment_amount == 0`: return full `escrow_lamports` to client
- Close Task account, return rent to client
- Set `state = Finalized` (momentary, account closes immediately)
- Emit `TaskFinalized { task_id, agent, payment_amount, fee_amount }`

#### `challenge_task`
Signers: `challenger`
- Assert `state == Verified`
- Assert `Clock::now() < challenge_deadline`
- Compute required bond: `2-5x escrow_lamports` (full task price, not computed payout). All challengers (including the client) pay the standard bond.
- Init Challenge PDA
- Transfer bond from challenger to Challenge PDA
- Set `state = Disputed`
- Emit `TaskChallenged { task_id, challenger, bond_lamports, is_client_challenge }`

#### `resolve_challenge`
Signers: `authority` (Squads multisig)
- Assert `state == Disputed`
- Input: `challenger_won: bool`
- If challenger won: return escrow to client, return bond to challenger, agent gets $0
- If agent won: release `payment_amount` to agent, slash challenger's bond (portion to agent, portion to treasury), return remainder escrow to client
- Close Task and Challenge accounts
- Emit `ChallengeResolved { task_id, challenger_won, bond_slashed }`

#### `expire_task`
Permissionless crank — anyone can call.
- Assert `state == Open || state == Claimed` AND `Clock::now() > deadline`
- OR assert `state == Submitted` AND `Clock::now() > submitted_at + VERIFICATION_TIMEOUT` (T+14d)
- Return `escrow_lamports` to client
- Close Task account, return rent to client
- Emit `TaskExpired { task_id, state_at_expiry }`

#### `emergency_return`
Signers: `authority` (Squads multisig only)
- Accepts a list of Task accounts as remaining accounts
- For each: assert `state == Open || state == Claimed`
- Return `escrow_lamports` to each task's client
- Close each Task account
- Emit `EmergencyReturn { task_ids: Vec<u64> }`

Used when the platform becomes unavailable (e.g., YouTube API crackdown). Does NOT affect Submitted/Verified/Finalized tasks — those are handled by the verification timeout (T+14d).

#### `revoke_session`
Signers: `agent` (the delegating agent, NOT the session key)
- Close the `SessionDelegate` PDA for the given delegate pubkey
- Returns rent to agent
- Emit `SessionRevoked { agent, delegate }`

Allows agents to instantly revoke MCP server session delegation if the server is compromised.

### Constants

```rust
pub const CHALLENGE_WINDOW_SECONDS: i64 = 86_400;    // 24 hours
pub const VERIFICATION_TIMEOUT_SECONDS: i64 = 1_209_600; // 14 days
pub const MAX_CONCURRENT_CLAIMS: u8 = 5;
pub const MAX_SCORE: u64 = 1_000_000;                 // fixed-point 1e6
pub const MIN_CLAIM_BUFFER_SECONDS: i64 = 14_400;     // 4 hours
pub const MIN_CHALLENGE_BOND_MULTIPLIER: u8 = 2;       // 2x full task price
pub const MAX_CHALLENGE_BOND_MULTIPLIER: u8 = 5;       // 5x full task price
```

### Error Types

```rust
InvalidTaskState              // instruction not valid for current state
NotTaskClient                 // caller is not the task's client
NotTaskAgent                  // caller is not the task's agent
NotAuthority                  // caller is not the GlobalState authority
DeadlineExpired               // claim/submit attempted after deadline
ClaimBufferInsufficient       // not enough time remaining to claim
SubmitMarginInsufficient      // submission too close to deadline
MaxConcurrentClaimsExceeded   // agent has 5+ active claims
InvalidAttestation            // Switchboard account ownership or feed PDA mismatch
AttestationStale              // oracle data outside acceptable window
ScoreOutOfBounds              // composite score exceeds MAX_SCORE
ChallengeWindowClosed         // challenge attempted after window expired
ChallengeWindowOpen           // finalize attempted before window closes
InsufficientBond              // challenge bond below minimum
VerificationTimeoutNotReached // expire called on Submitted task before T+14d
InvalidSessionDelegate        // session key not authorized for this instruction
ArithmeticOverflow            // checked arithmetic failure
PaymentExceedsEscrow          // payment + fee > escrow (invariant violation)
```

### Events

```rust
TaskCreated       { task_id: u64, client: Pubkey, escrow_lamports: u64, deadline: i64, task_nonce: [u8; 16] }
TaskClaimed       { task_id: u64, agent: Pubkey }
WorkSubmitted     { task_id: u64, agent: Pubkey, video_id_hash: [u8; 32] }
TaskVerified      { task_id: u64, composite_score: u64, payment_amount: u64 }
TaskFinalized     { task_id: u64, agent: Pubkey, payment_amount: u64, fee_amount: u64 }
TaskChallenged    { task_id: u64, challenger: Pubkey, bond_lamports: u64, is_client_challenge: bool }
ChallengeResolved { task_id: u64, challenger_won: bool, bond_slashed: u64 }
TaskExpired       { task_id: u64, state_at_expiry: u8 }
EmergencyReturn   { task_ids: Vec<u64> }
SessionRevoked    { agent: Pubkey, delegate: Pubkey }
```

### Immutable Invariants (hardcoded, no key can change)

These are enforced at the program level and cannot be modified by governance, multisig, or upgrade:

1. Escrow release requires a valid Switchboard oracle attestation
2. Attestation accounts must be owned by the Switchboard program and derived from the correct feed PDA (feed address derived from fixed seeds, not mutable config)
3. Video description must contain the task nonce for verification to pass (verified off-chain by the oracle function, hash of nonce included in attestation)
4. Payment requires composite score >= quality threshold
5. Challenge window must exist (CHALLENGE_WINDOW_SECONDS > 0)
6. Verification timeout at T+14d returns escrow if no attestation received
7. Strict state machine enforcement — every instruction asserts valid source states
8. CEI ordering — all state mutations before any CPI
9. `payment_amount + fee <= escrow_lamports` asserted before every transfer

### Parameter Governance

The Squads multisig (v1) or Realms DAO (future) can modify these parameters via `update_params` instruction:

- `protocol_fee_bps` — within bounds [100, 2500] (1-25%)
- `quality_threshold` — within bounds [MIN_THRESHOLD, MAX_THRESHOLD] (set by multisig)
- `scoring_weights` — within bounds [500, 5000] per weight in basis points (0.05-0.50), must sum to 10000

The bounds themselves are controlled by the Squads multisig (not governance) to prevent governance attacks from widening parameter ranges.

### Upgrade Authority

- **Devnet:** EOA (fast iteration)
- **Mainnet:** Squads multisig with minimum 48-hour timelock. Automated monitoring alerts on any upgrade buffer initialization.
- **CI check:** Deployment scripts assert upgrade authority matches expected Squads address on mainnet. Fail hard if EOA.
- **Long-term:** Consider making the program immutable after a stability period (6+ months of no critical upgrades).

---

## Crate: shared (Library, NOT a Program)

Platform-agnostic types used by both on-chain programs and off-chain services.

### Types

```rust
/// Platform-agnostic proof of content existence
pub struct PlatformProof {
    pub platform: PlatformType,
    pub content_id_hash: [u8; 32],
    pub nonce: [u8; 16],
    pub timestamp: i64,
}

pub enum PlatformType {
    YouTube = 0,
    Farcaster = 1,   // future
    TikTok = 2,      // future
}

/// Platform-agnostic engagement metrics
pub struct EngagementMetrics {
    pub views: u64,
    pub likes: u64,
    pub comments: u64,
    pub shares: u64,
    pub engagement_rate_bps: u64,  // engagements/views in basis points
}

/// Composite score with breakdown
pub struct CompositeScore {
    pub total: u64,                // fixed-point, max = MAX_SCORE
    pub metric_scores: [u64; 6],   // per-metric weighted scores
    pub penalty: u64,              // bot engagement penalty applied
}

/// Scoring weight configuration
pub struct ScoringWeights {
    pub weights: [u16; 6],         // basis points per metric, must sum to 10000
    pub penalty_weight: u16,       // bot penalty weight in basis points
}
```

These types are used by the shillbot on-chain program for attestation validation and by the off-chain scorer/verifier services for computation.

---

## Program: coordination (Detailed Specification)

The full coordination game specification follows. The canonical game design document is at `coordination/coordination-game/CLAUDE.md` — read it for game theory rationale, player psychology, and economic model. This section covers the on-chain implementation.

### Account Structures

#### `GameCounter`
Singleton PDA. Seeds: `["game_counter"]`

```
discriminator:    8 bytes
count:            u64      incremented on each create_game; used as game_id
bump:             u8
```

#### `GlobalConfig`
Singleton PDA. Seeds: `["global_config"]`

```
authority:            Pubkey    governance authority (EOA for v1, Squads multisig later)
matchmaker:           Pubkey    authorized matchmaker that gates create_game
treasury:             Pubkey    DAO treasury address for losing stake split
treasury_split_bps:   u16       portion of losing stakes sent to treasury (basis points, default 5000 = 50%)
bump:                 u8
```

Bounds: `treasury_split_bps` must be in [2000, 8000] (20-80% treasury share).

#### `Tournament`
PDA seeds: `["tournament", tournament_id: u64 as 8-byte LE]`

```
tournament_id:            u64
authority:                Pubkey    informational only — no admin power post-creation
start_time:               i64       Unix timestamp
end_time:                 i64       Unix timestamp
prize_lamports:           u64       accumulates tournament share of losing stakes
game_count:               u64       total resolved games (ALL resolved games, not just those with pool gain)
finalized:                bool      set by finalize_tournament after end_time
prize_snapshot:           u64       prize_lamports frozen at finalization
merkle_root:              [u8; 32]  root of merkle tree of (player, entitlement) pairs, set at finalization
bump:                     u8
```

The Tournament PDA is both data store and lamport vault for the prize pool.

#### `PlayerProfile`
PDA seeds: `["player", tournament_id: u64 as 8-byte LE, wallet: Pubkey]`

One profile per (wallet, tournament). Created at `join_game` via `init_if_needed`; player pays for creation.

```
wallet:           Pubkey
tournament_id:    u64
wins:             u64      games where player guessed correctly (including homogeneous both-correct for BOTH players)
total_games:      u64      resolved games in this tournament
score:            u64      wins * wins / total_games (updated after each resolution)
claimed:          bool     set to true after claim_reward; prevents double-claim
bump:             u8
```

**Win definition:** A player earns a win when they guess correctly. In homogeneous both-correct, BOTH players earn a win. In heterogeneous matches, the player who takes the pot earns a win.

#### `Game`
PDA seeds: `["game", game_id: u64 as 8-byte LE]`

```
game_id:                  u64
tournament_id:            u64
player_one:               Pubkey
player_two:               Pubkey    zero-key until second player joins
state:                    u8        GameState enum
stake_lamports:           u64       per-player stake; set at creation, must match exactly at join
p1_commit:                [u8; 32]  SHA-256 commitment, zeroed until set
p2_commit:                [u8; 32]
p1_guess:                 u8        0 = same team, 1 = different team, 255 = unrevealed
p2_guess:                 u8        0 = same team, 1 = different team, 255 = unrevealed
first_committer:          u8        0 = neither, 1 = p1, 2 = p2
p1_commit_slot:           u64       Solana slot at commit time
p2_commit_slot:           u64       Solana slot at commit time
commit_timeout_slots:     u64       set at creation
created_at:               i64       Unix timestamp
resolved_at:              i64       0 until resolved
activated_at_slot:        u64       Solana slot when game entered Active (set by join_game)
matchup_commitment:       [u8; 32]  SHA-256 commitment of matchup type preimage (set at create_game)
matchup_type:             u8        0 = same team, 1 = different teams, 255 = unset (resolved at first reveal)
bump:                     u8
```

The Game PDA holds both players' staked lamports in its own balance.

### State Machine

```
         ──(create_game)──► Pending
Pending ──(join_game)──► Active
Active ──(commit_guess: first)──► Committing
Active ──(resolve_timeout: neither commits)──► Resolved (both forfeit)
Committing ──(commit_guess: second)──► Revealing
Committing ──(resolve_timeout)──► Resolved
Revealing ──(reveal_guess: both revealed)──► Resolved
Revealing ──(resolve_timeout)──► Resolved
Resolved ──(close_game)──► [account closed]
```

### Instructions

#### `initialize`
Creates the global `GameCounter` PDA. One-time setup.

#### `initialize_config`
Signers: `authority`. Creates the `GlobalConfig` singleton PDA with authority, matchmaker, treasury, and treasury_split_bps. Validates treasury_split_bps within [2000, 8000].

#### `update_config`
Signers: `authority` (must match `global_config.authority`). Updates treasury, treasury_split_bps, matchmaker, or authority. Validates bounds.

#### `create_tournament`
Signers: any wallet. Validate `end_time > start_time` and `end_time > Clock::now()`. Initialize Tournament PDA.

#### `create_game`
Signers: `player` (payer) + `matchmaker` (co-signer, must match `global_config.matchmaker`). Player calls this with a `matchup_commitment` (SHA-256 hash of the matchup type preimage) provided by the backend. The matchmaker co-signs to attest the commitment is legitimate — player pays all gas, matchmaker pays nothing. Rejects all-zero commitments. Assert within tournament window. Assert end-of-tournament cutoff (`now + COMMIT_TIMEOUT_SLOTS + REVEAL_TIMEOUT_SLOTS < tournament.end_time`). Validate and consume player escrow. Init Game PDA with `matchup_type = 255` (unset).

#### `join_game`
Signers: player (becomes player two). Assert `state == Pending`, player != player_one, within tournament window. Transfer stake from player's escrow to Game PDA. Transition to Active.

#### `commit_guess`
Signers: participant. Assert Active or Committing. Store commitment hash. Record commit slot and first_committer.

#### `reveal_guess`
Signers: participant. Parameters: `r: [u8; 32]` (guess preimage), `r_matchup: Option<[u8; 32]>` (matchup preimage). Assert Revealing. Verify `SHA-256(r) == commitment`. Extract `guess = r[31] & 1`. If `matchup_type == 255` (unset), require `r_matchup`, verify `SHA-256(r_matchup) == matchup_commitment`, extract `matchup_type = r_matchup[31] & 1`. If both revealed, resolve game. Split `tournament_gain` between treasury and prize pool. Increment `game_count` for ALL resolved games. Award wins based on correct guesses.

#### `resolve_timeout`
Permissionless. Handles three states:
- **Active (neither committed):** Both forfeit. Stakes go to pool/treasury split.
- **Committing (one committed):** Committer wins the full pot (2S). Non-committer is slashed.
- **Revealing (one revealed):** Revealer receives the full pot (2S). Non-revealer is slashed. (This prevents timeout griefing — a loser refusing to reveal at zero extra cost to themselves.)

#### `close_game`
Permissionless. Assert Resolved. Close account, return rent to caller.

#### `finalize_tournament`
Signers: `authority` (must match `global_config.authority`). Authority-gated to prevent selective inclusion attacks. Assert past end_time and not finalized. Authority posts the 32-byte merkle root of (player, entitlement) pairs computed off-chain. Sets `finalized = true` and `prize_snapshot`.

#### `claim_reward`
Signers: player. Assert finalized, not claimed, minimum 5 games. Player submits merkle proof. On-chain: verify proof against stored `merkle_root` using `keccak::hashv` with domain separation (0x00 prefix for leaves, 0x01 for internal nodes, sorted children). Transfer entitlement from tournament PDA. Set `claimed = true`.

### Payoff Logic

**Same team (matchup_type = 0):**
- Both correct → both keep stake (S, S). Tournament gains 0. Both players earn a win.
- One correct, one wrong → correct gets 0.5S, wrong gets 0. Tournament gains 1.5S. Correct player earns a win.
- Both wrong → both forfeit. Tournament gains 2S.

**Different teams (matchup_type = 1):**
- One correct → winner takes full pot (2S, 0). Tournament gains 0. Winner earns a win.
- Both correct → first committer takes full pot (2S, 0). Tournament gains 0. Winner earns a win.
- Both wrong → both forfeit. Tournament gains 2S. (This prevents "always guess Same" collusion.)

**Tournament gains are split:** treasury gets `tournament_gain * treasury_split_bps / 10_000`, prize pool gets the remainder. Split happens in reveal_guess and resolve_timeout.

All arithmetic uses `checked_mul`/`checked_div`. Lamport conservation asserted: `p1_return + p2_return + tournament_gain == 2 * stake_lamports`.

### Timeouts

| Stage | Timeout |
|---|---|
| Active (neither commits) | 7,200 slots (~1 hour) |
| Committing | 7,200 slots (~1 hour) |
| Revealing | 14,400 slots (~2 hours) |

### Error Types

```rust
InvalidGameState, InvalidStateTransition, NotAParticipant, AlreadyCommitted,
AlreadyRevealed, AlreadyClaimed, CannotJoinOwnGame, StakeMismatch,
CommitmentMismatch, TimeoutNotElapsed, InvalidTournamentTimes,
TournamentNotEnded, TournamentNotFinalized, EmptyPrizePool,
OutsideTournamentWindow, ProfileTournamentMismatch, BelowMinimumGames,
ArithmeticOverflow, TooManyAccounts, NotAuthority, NotMatchmaker,
InvalidTreasurySplitBps, InvalidMerkleProof
```

### Events

```rust
TournamentCreated, GameCreated, GameStarted, GuessCommitted, GuessRevealed,
GameResolved { ..., treasury_gain: u64 }, TimeoutSlash, TournamentFinalized,
RewardClaimed, ConfigInitialized, ConfigUpdated
```

---

## Testing

**Unit tests:** Every pure function (payoff resolution, scoring computation, payment calculation) must have exhaustive unit tests in `#[cfg(test)]` modules. Test boundary conditions: zero values, maximum values, overflow scenarios.

**End-to-end tests:** Full instruction flows against a local validator using Anchor's test harness. Cover:
- Every instruction in every valid state
- Every error path (invalid state transitions, unauthorized callers, expired deadlines)
- Every state transition in the state machine
- The full task lifecycle: create -> claim -> submit -> verify -> finalize
- Challenge flows: challenge -> resolve (challenger wins), challenge -> resolve (agent wins)
- Timeout flows: expire from Open, Claimed, and Submitted states
- Emergency return with multiple tasks
- Session delegation and revocation
- Concurrent claim limit enforcement

**CI runs unit tests only** (no network calls). End-to-end tests are local system verification.

---

## Open Questions

- **Switchboard Function implementation** — the custom Switchboard Function that calls YouTube Data API v3, computes composite score, and posts attestation needs to be specified and built. This is off-chain code that runs in Switchboard's TEE environment.
- **`emergency_return` does not decrement AgentState.claimed_count** — when the multisig emergency-returns Claimed tasks, the affected agents' `AgentState.claimed_count` is not decremented. This is conservative (prevents over-claiming) but means agents may temporarily be unable to claim their full quota. The count self-corrects as other tasks are submitted or expired.
