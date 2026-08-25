# Mainnet deploy policy & allowlist

**Mainnet deploy triggers are per-program, not uniform.** The two revenue
programs (`coordination_game`, `shillbot`) auto-upgrade mainnet on every merge
to `main` that touches them and passes tests — safe specifically because Solana
programs are upgradeable in place (`solana program deploy` swaps bytecode and
preserves every PDA's state). The extension programs remain
`workflow_dispatch`-only. The trigger for each program lives in its workflow's
`deploy-mainnet` job `if:` condition; this table mirrors those conditions:

| Program | Workflow | deploy-devnet trigger | deploy-mainnet trigger |
|---|---|---|---|
| `coordination_game` | `coordination-game.yml` | `workflow_dispatch` (target=devnet) only | **Auto on push to `main`** (path-filtered) after `test` passes; also `workflow_dispatch` (target=mainnet). Uses the `mainnet` GitHub environment. |
| `shillbot` | `shillbot-program.yml` | **Auto on push to `main`** (keeps devnet in lockstep with mainnet — see the a685cdf drift note in the workflow); also dispatch (target=devnet) | **Auto on push to `main`**, staged `test → deploy-devnet → deploy-mainnet` (mainnet only runs after devnet deploys cleanly); also `workflow_dispatch` (target=mainnet, devnet skipped). |
| `extension_registry` | `extension-registry-program.yml` | `workflow_dispatch` (target=devnet) only | `workflow_dispatch` (target=mainnet) only. Uses the `mainnet` GitHub environment. |
| `extension_credit` | `extension-credit-program.yml` | `workflow_dispatch` only | **No mainnet job** (devnet-only program). |

Auto-deploy on push means: a red test suite blocks the upgrade (`needs: test`),
and for shillbot a failed devnet deploy also blocks mainnet. The shillbot
mainnet deploy step reads the premium RPC from GCP and fails loud (red CI) if
the RPC read or deploy fails — it never silently falls back to the flaky public
endpoint (the a685cdf lesson).

If you change a `deploy-mainnet` trigger, update this table in the same commit.

## Cleared for mainnet

**The canonical allowlist is `Anchor.toml`'s `[programs.mainnet]` section.**
`anchor deploy --provider.cluster mainnet` can only deploy a program that has an
ID configured there, so a program absent from `[programs.mainnet]` cannot be
deployed to mainnet at all. To make a new program mainnet-eligible you must add
it to `[programs.mainnet]` (a reviewable diff). This table mirrors that list and
adds status:

| Program | Program ID | Status |
|---|---|---|
| `coordination_game` | `2qqVk7kUqffnahiJpcQJCsSd8ErbEUgKTgCn1zYsw64P` | ✅ Live. Auto-upgrades on merge to `main`. |
| `shillbot` | `2tR37nqMpwdV4DVUHjzUmL1rH2DtkA8zrRA4EAhT7KMi` | ✅ Live. Auto-upgrades on merge to `main` (staged behind devnet). LeanProof bounties have paid out on mainnet; the daily `mainnet-smoke.yml` preflight checks the earn→verify→pay invariants. The client-nonce `create_task` change (a685cdf) is deployed. **Open item:** the C2 `Task.payout_to` extension (315 → 347-byte Task layout, realloc path) shipped without the originally-planned external settlement audit — the freeze that used to gate this was overtaken by the mainnet launch. If a settlement audit happens, C2 is in scope. |
| `extension_registry` | `H7whziapWzGDH1b3QQzxno69TD4braekyBZhfjNGof4j` | Cleared 2026-07-08 (founder call: the credit_web signal goes real on mainnet). Mainnet deploy is `workflow_dispatch`-only; its `deploy-mainnet` job also runs the idempotent `GlobalState` initialize (authority = treasury = root wallet). |

## Devnet-only (in development — never mainnet)

| Program | Program ID |
|---|---|
| `extension_credit` | `Ec628D7GH3hwgnVf1gqrUh83qcZprYjmTzvtdHHEr7oh` |

These have **no `deploy-mainnet` job**. They run on devnet for the
extension-credit dogfood and are not mainnet-eligible. Adding a mainnet job for a
new program requires adding it to the "cleared" table above first.

## Manual dispatch (extension programs, or out-of-band deploys)

Only for a program in the **Cleared** table, after its changes are reviewed:

```
gh workflow run extension-registry-program.yml --ref main -f target=mainnet
```

or trigger `deploy-mainnet` from the Actions UI (select `target=mainnet`).
Dispatch also works for the auto-deploying programs when you need a deploy
without a code change (e.g. re-running after a transient RPC failure).

### `coordination_game` builds mainnet WITH `--features mainnet`

A Solana program binary can't observe its own cluster at runtime, but the
cross-chain leg-A chain tag (`solana_chain_tag`) must be this cluster's genesis.
So the cluster is a **build-time** feature: the `deploy-mainnet` job builds
`anchor build -p coordination-game -- --features mainnet` (mainnet Solana
genesis); every other build (test job, devnet job, local) omits it (devnet
genesis). **If a mainnet build ever ships without `--features mainnet`, all
cross-chain settlement breaks with `XCertMismatch`** — the same-chain game is
unaffected (the tag is cross-chain-only). This is enforced only by the workflow
step; keep the flag on that step.

## Devnet SOL

Devnet deploys consume the (rate-limited) devnet faucet. `coordination_game`,
`extension_registry`, and `extension_credit` therefore keep their
`deploy-devnet` jobs `workflow_dispatch`-only. `shillbot` is the exception: its
devnet job auto-runs on push so devnet never drifts behind the auto-deployed
mainnet (the a685cdf `InstructionDidNotDeserialize` regression), and it deploys
in place via `solana program deploy` (buffer rent only, auto-reclaimed). During
development the new programs are deployed to devnet manually
(`solana program deploy ... --url devnet`).
