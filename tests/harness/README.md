# Modular, verification-first e2e harness

A small toolkit for writing e2e/integration scenarios as **composable steps** whose
verification is **path-independent** — so you can combine steps freely and still
verify the result even though the end state varies with the composition.

## The core idea: verify properties, not a fixed end state

When scenarios are built by combining steps, the end state isn't known up front.
So nothing here asserts a hardcoded outcome. Instead every step records what
**actually happened** into two ledgers, and the scenario ends by running a
**property battery** computed from those realized records:

- `Ledger` (`ledger.ts`) — opening/closing balances of every value-bearing account
  the scenario touched (`protocol` vs `player`).
- `Transcript` (`ledger.ts`) — the realized game events (commits, reveals, matchup),
  which produce the shared oracle's input via `toOracleInput()`.

### The property battery (`assertions.ts`)

| # | Check | What it proves (for any composition) |
|---|-------|--------------------------------------|
| 1 | `assertConservation` | Σ(all tracked deltas) == −fees. Value is never created/destroyed (handles payoffs, forfeits, cross-chain float alike). |
| 1’ | `assertPayoffMatchesLedger` | same-chain only: protocol take == oracle `tournamentGain` for the realized transcript. |
| 2 | `assertOracleOutcome` | on-chain outcome == `deriveClaimOutcome(realized transcript)` — derived, never assumed. |
| 3 | `assertLegalTransition` / `assertTerminal` | every observed status transition is in the allowed graph; terminals are absorbing. |
| 4 | `assertCrossLayer` | independent views (on-chain ↔ backend ↔ oracle) agree — catches integration drift (e.g. a backend lagging the chain). |
| 5 | `assertMetamorphic` | the same scenario on two runtimes reaches the same outcome + net — verification without a fixed answer. |
| 6 | `assertNonDecreasing` / `assertStrictlyIncreasing` | monotonic bounds (e.g. supersede `best_step_count` strictly increases). |

## The seam: Targets + StateView

A **step** runs against a `SolanaRuntime` (`target.ts`) — the runtime seam. The
property battery reads realized state through a `StateView` (`readStatus`,
`readOutcomeKind`). Implementations today:

- `bankrun.ts` — in-process Solana VM (owns the clock → `warpTo`); CI-safe.
- *validator* — local-validator `SolanaRuntime` (follow-up; `warpTo` unsupported).
- `evm-target.ts` — StateViews over the deployed EVM CoordinationGame (reads injected → unit-testable; live script supplies viem).
- `backend-target.ts` — phase StateView over game-api `/internal/*` (reads injected).

Steps are grouped by domain: `game-steps.ts` (same-chain create/join/commit/reveal),
`xchain-steps.ts` (cross-chain lock/open/supersede/settle). Each step performs ONE
action and asserts its local transition; scenarios compose them.

## What runs where

- **`*.unit.test.ts`** (pure: battery, adapters, mappings) → `harness-unit.yml`, fast, no validator.
- **bankrun scenarios** (`tests/xchain-contested.ts`, `tests/game-harness.ts`) → `coordination-game.yml` bankrun step, no validator.
- **validator scenarios** (`tests/coordination-game.ts`, `tests/xchain.ts`) → `coordination-game.yml` validator step.
- **live scenarios** (`tests/live/*`) → manual (real testnet gas); wire the EVM/backend Targets to real viem/fetch readers.

> **Local gating note:** the repo's chain tests transitively import `@noble`, which
> ESM-flips mocha on node ≥ 22. Gate chain/bankrun suites under **node 20**:
> `export NVM_DIR=$HOME/.nvm; . $NVM_DIR/nvm.sh; nvm use 20` then `npx ts-mocha …`.

## Adding a new product (the reserved plug-in contract)

A new product plugs in **additively** — implement a `StateView` + steps, reuse the
battery. Reserved vocabularies (not yet built):

- **shillbot:** `createTask → claimTask → submitWork → verifyTask → finalizeTask`
  (+ `challenge`), with a `taskView` StateView and a TASK_GRAPH. The battery applies
  directly: conservation over escrow/treasury/agent, oracle = the scoring/payout
  rule, legality over the task state machine, cross-layer (orchestrator ↔ chain).
- **browser (coordination-game / shillbot frontends):** `connect → joinQueue →
  commit → reveal` driven through Playwright as a `Target`, with a page-object
  `StateView`; metamorphic against the same scenario on bankrun.

Keep new product logic in its own `*-steps.ts` + `*-target.ts`; never weaken a
battery check to fit a product — extend the catalog instead.
