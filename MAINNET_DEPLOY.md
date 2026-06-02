# Mainnet deploy policy & allowlist

**Real SOL is only spent on deliberate, manual mainnet deploys.** No workflow
auto-deploys to mainnet on push — every `deploy-mainnet` job is gated to
`workflow_dispatch` (manual). This stops churning real SOL on every commit and
prevents unreviewed/unaudited changes from reaching mainnet just because they
landed on `main`.

(Previously both `shillbot-program.yml` and `coordination-game.yml` deployed to
mainnet on **every** push to `main` that passed tests — that's the churn this
policy removes.)

## Cleared for mainnet

**The canonical allowlist is `Anchor.toml`'s `[programs.mainnet]` section.**
`anchor deploy --provider.cluster mainnet` can only deploy a program that has an
ID configured there, so a program absent from `[programs.mainnet]` cannot be
deployed to mainnet at all. To make a new program mainnet-eligible you must add
it to `[programs.mainnet]` (a reviewable diff). This table mirrors that list and
adds status:

| Program | Program ID | Status |
|---|---|---|
| `coordination_game` | `2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P` | ✅ Live. Deploy manually when releasing a reviewed change. |
| `shillbot` | `2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi` | ⚠️ **FROZEN.** `main` carries the C2 `Task.payout_to` change (347‑byte Task layout). It is **not** cleared for mainnet: it needs the settlement audit, and would break existing 242/244/315‑byte Task accounts without a migration path for every legacy size. **Do not dispatch a shillbot mainnet deploy until C2 is audited + that migration ships.** |

## Devnet-only (in development — never mainnet)

| Program | Program ID |
|---|---|
| `extension_registry` | `H7whziapWzGDH1b3QQzxno69TD4braekyBZhfjNGof4j` |
| `extension_credit` | `GJLUpJHceGekHBeZMZX4ZYX4xdkK4kFw2tH6uRuQHDqm` |

These have **no `deploy-mainnet` job**. They run on devnet for the
extension-credit dogfood and are not mainnet-eligible. Adding a mainnet job for a
new program requires adding it to the "cleared" table above first.

## How to deploy to mainnet (when intended)

Only for a program in the **Cleared** table, after its changes are reviewed:

```
gh workflow run "Coordination Game" --ref main   # then the deploy-mainnet job
```

or trigger `deploy-mainnet` from the Actions UI. The `test` job remains the
automatic push gate; deploys are always an explicit, separate action.

## Devnet SOL

Devnet deploys also consume the (rate-limited) devnet faucet, so the program
`deploy-devnet` jobs are likewise `workflow_dispatch`-only — deploy to devnet
intentionally, not on every push. During development the new programs are
deployed to devnet manually (`solana program deploy ... --url devnet`).
