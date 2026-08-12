# Surface parity: where Solana and EVM differ, and whether anyone decided that

The Coordination Game runs on three surfaces — Solana same-chain, EVM same-chain
(Base + Ethereum), and cross-chain. They are meant to behave identically except
where the chains force otherwise.

This file exists because the difference between "structural" and "nobody
decided" was tribal knowledge, and the cost of not writing it down is on the
record: a registry comment claiming "neither deployed client passes the optional
`global_config` account" stayed there after it stopped being true, and it reads
as standing permission to re-pin a chain to a compile-time constant — the exact
move that produced a $3.64 Solana stake against a $5 EVM anchor.

**Every row here was verified against the source or the chain on 2026-08-12, not
inferred from comments.** Where a claim came from running something, the command
is named.

Legend: **STRUCTURAL** = the chains force it, do not "fix" · **UNIFIED** = held
equal, with the guard named · **OPEN** = real divergence, nobody chose it

---

## Money

| Behaviour | Solana | EVM | Status |
|---|---|---|---|
| Payoff matrix | `chain_core::game::amounts_for_kind` | `CoordinationGame.sol::_amounts` | **UNIFIED** — one definition; exhaustive golden vectors over all 9 outcome kinds (`crates/chain-core/tests/game_payout_vectors.rs`) |
| Stake intake | `deposit_stake`, a separate tx **before** pairing | inline `msg.value` at `createGame`/`joinGame` | **STRUCTURAL** |
| Stake source | `GlobalConfig.stake_lamports`, read live per deposit | `stakeWei` immutable in the contract | **STRUCTURAL** |
| Re-peg cost | one instruction (`set_stake_lamports`) | one tx (`setConfig`) | **UNIFIED** — was asymmetric (Solana needed a *program upgrade*), which is why it drifted; both are now cheap and CI-converged by `Reconcile {EVM,Solana} Stake` |
| Wrong/missing price | falls back to compile-time `DEFAULT_STAKE_LAMPORTS`, deposit succeeds, rejected one step later at `create_game` (`StakeMismatch 0x1776`) — **after** escrow | `msg.value != stakeWei` reverts at the door | **OPEN** (see §Open 1) |
| Per-match quote | cross-chain only (`create_xmatch` takes `stake_lamports`) | none — contract-fixed | **STRUCTURAL** (see §Open 3) |

**Why stake intake is structural, and why it blocks per-match pricing:** the
Solana deposit happens *before* a match exists, so there is nothing to quote
against. Of the four game surfaces, exactly one (`create_xmatch`) can carry a
per-match amount; the other three are pinned to a contract or program config.
That is why ETH is the anchor and Solana is the surface that follows —
`STAKE_ANCHOR_WEI` in `crates/chain-registry/src/lib.rs`, which spells out that
only the SOL/ETH *ratio* matters because USD cancels.

**Anchor drift is expected and is not a bug.** The Solana literal is the anchor
converted at the ratio *on the peg date*; the ratio moves and the config does
not. Measured via `node tests/e2e/scripts/check-stake-parity.mjs`: +2.9%
(~$0.15) on 2026-08-12, inside the script's ±5% band. Re-peg on a band, not on
every tick — each re-peg is a transaction that can strand an in-flight deposit.

## Lifecycle

| Behaviour | Solana | EVM | Status |
|---|---|---|---|
| Timeout resolution | `resolve_timeout` — `TimeoutNotElapsed` | `resolveTimeout` — `_requireElapsed` per phase (Active/Committing/Revealing, distinct anchors + windows) | **UNIFIED** |
| Abandoned pairing | `refund_pending` | `cancelPending` | **UNIFIED** — deliberately mirrored |
| Prize claim | `claim_reward` (merkle proof) | `claimPrize` | **UNIFIED** |
| Unclaimed sweep | `sweep_unclaimed` (authority + grace period) | `sweepUnclaimed` | **UNIFIED** |
| Season/tournament close | `finalize_tournament` | `finalizeSeason` | **UNIFIED** |
| Account rent | `close_game`, `withdraw_stake` reclaim rent | n/a | **STRUCTURAL** — Solana rent has no EVM analogue |
| Emergency pause | **none** | `Pausable` on `createGame`/`joinGame`; resolution and refund deliberately left unpaused (`CoordinationGameV4.sol:274,322,702`) | **OPEN** (see §Open 2) |
| Session model | per-instruction `*_session` variants (9 files) | `openSession` + `sessionAuthDigest` + `revokeSession` | **OPEN, low** — same capability, different shape; no evidence anyone compared them |

## What the player is told

| Behaviour | Solana | EVM | Status |
|---|---|---|---|
| Stake quoted before staking | live `GlobalConfig` read (`useLiveStake`) | derived from the registry export, shown on the panel and the fund button | **UNIFIED** as of `324ea61c` — EVM previously showed **no amount at all** |
| Payout table | derived from the live stake | n/a (no EVM payout table) | — |
| Client-side stake verification | none | quorum read of `stakeWei` across RPCs; refuses to fund on disagreement (`xchain-tx.ts:156-166`) | **STRUCTURAL** — the Solana amount is program-determined, so the client never sends an amount; a bad read misquotes the *display*, never the *charge*. Do not "fix" this into a blocking read. |

---

## Open items

**1. Fail-open vs fail-closed on price.** EVM refuses at the door; Solana takes
the deposit and rejects at the next step, after the player's money is escrowed.
Recoverable (`withdraw_stake` only needs `amount > 0`) but it is a live outage,
and it has happened. Unifying means making `live_stake` require the
`global_config` account — a program change, so out of scope here.

*Currently unreachable, verified:* all three clients pass the account — frontend
`lib/anchor.ts` (pinned by `live-stake-account.test.ts`), `game-chain`
`instructions.rs::build_deposit_stake` (pinned by "deposit_stake must have 5
accounts"), and coordination-app's lockfile resolves `game-chain` to `7c849ea`,
which contains the append. The fallback protects no shipped client.

**2. Solana has no emergency pause.** EVM can stop new games while letting
in-flight ones resolve. On Solana the only lever is a program upgrade. Excluded
from the current scope by decision, recorded here because an incident is the
wrong time to discover it.

**3. Per-match live pricing on same-chain Solana.** The registry proposes
`deposit_stake` taking `stake_lamports`. **That does not work as written** — the
deposit precedes pairing, so there is no match to quote against. A real version
also needs matchmaking to bucket players by deposited amount (you cannot pair
0.0665 against 0.0685) and `create_game` to validate a band rather than
equality. The on-chain half is closer than expected: `create_game` already takes
`stake_lamports` and `join_game` already validates P2 against
`game.stake_lamports` (`join_game.rs:28,74`), so parity is per-game already —
only one `require!` in `create_game` pins it to the config.

---

## How these stay true

- **Payoff matrix** — golden vectors, exhaustive over the outcome domain.
- **Registry ↔ chain** — `Reconcile {EVM,Solana} Stake` converge on push and are
  idempotent ("PASS: chain already matches the registry" when there is nothing
  to do).
- **Registry ↔ clients ↔ UI** — `tests/e2e/scripts/check-stake-parity.mjs`, four
  layers plus a warn-only live-rate drift check. Deliberately **not** in CI: it
  makes RPC and price-feed calls, and it is a re-peg runbook tool. Its layer-3
  parser has a zero-parse tripwire that refuses to report success when the
  catalog format changes — it fired, correctly, when the UI strings were derived.
- **UI copies** — removed rather than checked. `chain-catalog.ts` derives every
  entry; `chain-catalog-stake.test.ts` fails on *any* literal amount, so the
  guard also covers chains added later.

When adding a surface or an instruction, add its row. A behaviour that appears
on one chain and not the other is a decision — make it here, in writing.
