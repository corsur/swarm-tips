# Smart Contracts — Code Standards & Workspace

Solana programs built with Anchor. For DAO overview and shared standards, see `swarm/CLAUDE.md`. For program-specific specs, see `programs/shillbot/CLAUDE.md` and `programs/coordination-game/CLAUDE.md`. For product context, see `swarm/shillbot/CLAUDE.md` and `swarm/coordination-game/CLAUDE.md`.

This workspace contains all on-chain programs for the Coordination DAO: the coordination game, the Shillbot task marketplace, and shared types/libraries.

---

## Workspace Structure

```
programs/
├── coordination-game/       # 1v1 commit-reveal game with tournaments
└── shillbot/                # Task marketplace with escrow + oracle verification

crates/
├── shared/                  # Platform-agnostic types (PlatformProof, EngagementMetrics, CompositeScore, ScoringWeights)
├── game-chain/              # Tx builder helpers: PDA derivation, instruction building, RPC client for coordination game
└── game-api-client/         # HTTP/WS client for off-chain game-api backend

services/
└── coordination-mcp-server/ # MCP server exposing tools for AI agents (see its own CLAUDE.md)

sdk/                         # TypeScript SDK (auto-generated Anchor IDL types)
tests/                       # End-to-end TypeScript tests against local validator
```

Instruction handlers must be thin — validate, delegate to a pure function, emit event. Business logic lives in pure functions, not handlers. Each instruction is its own file.

---

## Code Standards

### Design Philosophy

Write clean, minimal code. Complexity is a liability — every line of code is a line that can be wrong.

- **No speculative abstraction.** Don't create a generic `GameEngine` trait because "we might have more game types." Build the coordination game. Build the shillbot task lifecycle. If a pattern emerges across both, extract it then. The `shared` crate has the platform-agnostic types (`PlatformProof`, `EngagementMetrics`) — those are justified because changing on-chain types requires a program upgrade. Everything else starts concrete.
- **Delete, don't comment out.** No `// old payoff logic` or `// TODO: remove`. Git has the history. Commented-out instruction handlers are noise.
- **Names over comments.** `fn verify_commitment_matches_preimage(commitment: &[u8; 32], preimage: &[u8; 32]) -> bool` needs no comment. `fn check(c: &[u8; 32], r: &[u8; 32]) -> bool` needs a rewrite.
- **Flat over nested.** Use early returns with `require!()`: check all preconditions at the top of the handler, then the happy path is flat. If a handler has 3 levels of `if` nesting, break it up.
- **No clever code.** A `match` on `TaskState` is better than casting to `u8` and doing arithmetic on state values. The next auditor shouldn't have to decode your intent.
- **Refactor as you go.** When touching a file, fix naming, remove dead code, simplify structure. Leave every file cleaner than you found it.
- **Don't write tests until you know what behavior you want.** The spec defines the behavior. Tests encode the spec. A test that doesn't trace back to a spec requirement is worthless.
- **No worthless tests.** `assert!(create_task_works())` is not a test. A test that creates a task with specific parameters, then verifies the PDA state, escrow balance, and emitted event is a test.
- **No error swallowing.** Every failure mode must be visible. Every `require!()` uses a named error variant. Every error variant is documented. No `ProgramError::Custom(0)`.
- **Reject at system boundaries.** Every instruction handler validates ALL inputs before touching state. A game that accepts an invalid stake amount is worse than one that refuses a valid one.
- **Diagrams are mandatory.** Both state machines (game and shillbot) have ASCII diagrams in their respective CLAUDE.md files. When modifying state transitions, update the diagram FIRST.
- **Everything deferred is written down.** Open questions and deferred work live in the Open Questions section at the bottom of this file. Not as code comments.
- **Classify by reversibility.** On-chain decisions are almost all one-way doors: account structures, PDA seed derivation, state machine transitions, payment formulas. These require maximum rigor. The only two-way doors: parameter values (quality threshold, protocol fee, scoring weights) which are governance-adjustable within bounds. Treat everything else as irreversible.

### Rules — Anchor/Solana Specific

**Rule 1 — No recursion.** Solana BPF has a 4KB call stack. Recursive functions are banned. All iteration must be explicit and bounded.

**Rule 2 — Bounded loops.** Every loop iterating over instruction input must check its bound BEFORE entry:
```rust
require!(remaining_accounts.len() <= MAX_PLAYERS, TooManyAccounts);
for account_info in ctx.remaining_accounts.iter() {
    // safe: bounded by the check above
}
```
Never trust `remaining_accounts.len()` — an attacker chooses how many accounts to pass.

**Rule 3 — No unbounded resource consumption.** Collection sizes must be statically bounded. Never use `Vec::with_capacity(n)` where `n` comes from instruction input. Every PDA account has a fixed, known size. If a collection grows with usage, use separate PDA accounts per entry, not a growing Vec in one account.

**Rule 4 — Instruction handlers ≤100 lines.** Handlers validate, delegate to a pure function, and emit an event:
```rust
pub fn reveal_guess(ctx: Context<RevealGuess>, r: [u8; 32]) -> Result<()> {
    let game = &mut ctx.accounts.game;
    // Checks
    require!(game.state == GameState::Revealing, InvalidGameState);
    require!(!already_revealed(game, player), AlreadyRevealed);
    verify_commitment(game, player, &r)?;
    // Effects
    let guess = r[31] & 1;
    set_guess(game, player, guess);
    // Interactions
    if both_revealed(game) {
        resolve_game(game, &ctx.accounts.tournament, ...)?;
    }
    emit!(GuessRevealed { game_id: game.game_id, player: player.key() });
    Ok(())
}
```

**Rule 5 — Assert invariants (minimum 2 per function).** Every instruction handler: preconditions on entry, postconditions on exit. Pure functions called by handlers also assert.
```rust
// Precondition: correct state
require!(game.state == GameState::Verified, InvalidTaskState);
// ... do the work ...
// Postcondition: lamport conservation
let total_out = payment + fee + remainder;
require!(total_out == task.escrow_lamports, PaymentExceedsEscrow);
```

**Rule 6 — Smallest data scope.** Only request the accounts and permissions an instruction actually needs. Don't make accounts `mut` if the instruction only reads them.

**Rule 7 — No .unwrap() or .expect().** Every fallible call uses `?` or an explicit match. Use `ok_or(ErrorCode::ArithmeticOverflow)?` for Option types. Use `checked_add`, `checked_mul`, `checked_div`, `checked_sub` — never raw operators on values that could overflow.

**Rule 8 — No unsafe.** Zero `unsafe` in smart contract code.

**Rule 9 — Warnings as errors.** Every program crate:
```rust
#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::arithmetic_side_effects)]
```
`clippy::arithmetic_side_effects` rejects `+`, `-`, `*`, `/` on integers — you MUST use `checked_*` variants. Non-negotiable for code handling lamports, scores, and fees.

**Rule 10 — Release mode for production.** BPF builds are always release mode. Debug overflow checks don't exist in production. Every arithmetic safety check must be explicit (`checked_*`), never reliant on debug assertions. This is the #1 source of Solana exploits.

**Rule 11 — CEI (Checks-Effects-Interactions).** Instruction handlers follow this order exactly:
1. **Checks** — all `require!()` validations.
2. **Effects** — all `account.field = value` mutations.
3. **Interactions** — all lamport transfers and CPIs.
Never transfer lamports before state is committed. Never CPI before all account mutations are done.

### Solana Security Rules

- **Account ownership** — always verify an account is owned by the expected program before reading its data. Anchor typed accounts enforce this automatically. Never bypass with raw `AccountInfo` unless ownership is manually verified.
- **Signer checks** — never assume an account signed. Always verify via Anchor's `Signer` type or explicit `require!(account.is_signer)`.
- **PDA derivation** — always re-derive and verify PDA seeds on-chain. Never trust a PDA address passed in by a caller. Use Anchor's `seeds` and `bump` constraints.
- **State before CPI** — finalize ALL account state mutations before ANY cross-program invocation.

### Fixed-Point Arithmetic

All scoring and payment calculations use integer arithmetic with an explicit scaling factor (basis points with 10,000 denominator, or 1e6 for composite scores). Use u128 for intermediate products to prevent overflow. Assert `payment_amount + fee <= escrow_lamports` before any transfer.

### `init` vs `init_if_needed`

Use `init` exclusively for the shillbot program (prevents PDA account resurrection attacks), with one exception: `AgentState` uses `init_if_needed` (agent pays, no escrow funds, idempotent across first claim). The coordination game uses `init_if_needed` for `PlayerProfile` only (player pays, idempotent). Never use `init_if_needed` for accounts holding escrow funds.

### Observability

- **Events** — emit an Anchor event for every state transition, every payment, every slash, every challenge. Off-chain indexers consume these for monitoring.
- **`msg!()`** — use for debugging during development. Minimize in production (consumes compute units).
- **Named error variants** — every `require!` / error return uses a specific error from the program's error enum. Generic errors (`ProgramError::Custom(0)`) are banned.

### Deployment

`anchor deploy` from a local machine is forbidden on mainnet. All mainnet deployments go through CI with the upgrade authority check (assert Squads multisig, fail if EOA). Devnet deployments from local machines are acceptable during development.

---

## Testing

**Unit tests:** Every pure function (payoff resolution, scoring computation, payment calculation) must have exhaustive unit tests in `#[cfg(test)]` modules. Test boundary conditions: zero values, maximum values, overflow scenarios.

**End-to-end tests:** Full instruction flows against a local validator using Anchor's test harness. Cover:
- Every instruction in every valid state
- Every error path (invalid state transitions, unauthorized callers, expired deadlines)
- Every state transition in the state machine
- Full task/game lifecycles
- Challenge flows, timeout flows, emergency return
- Session delegation and revocation
- Concurrent claim limit enforcement

**CI runs unit tests only** (no network calls). End-to-end tests are local system verification.

---

## Open Questions

- **Switchboard Function implementation** — the custom Switchboard Function that calls YouTube Data API v3, computes composite score, and posts attestation needs to be specified and built. This is off-chain code that runs in Switchboard's TEE environment.
- **`emergency_return` does not decrement AgentState.claimed_count** — when the multisig emergency-returns Claimed tasks, the affected agents' `AgentState.claimed_count` is not decremented. This is conservative (prevents over-claiming) but means agents may temporarily be unable to claim their full quota. The count self-corrects as other tasks are submitted or expired.
