# Re-pegging an EVM stake

The stake exists in four places that must agree, or the chain is unplayable:

| where | what | who changes it |
|---|---|---|
| `crates/chain-registry/src/lib.rs` | `stake_base_units` | this repo |
| `.github/workflows/_deploy-evm.yml` | `XCHAIN_STAKE_WEI` | this repo |
| the deployed contract | `stakeWei` | **`setConfig`, on-chain** |
| `coordination-app` frontend + e2e | display + harness copies | the other repo |

Two of those are enforced automatically:

- `deploy_workflow_stakes_match_the_registry` (cargo test) — registry vs workflow
- `tests/e2e/scripts/check-stake-parity.mjs` (network) — registry vs **deployed chain**

## Why the order matters

`createGame` reverts `BadStake` unless `msg.value == stakeWei`, and the
cross-chain client reads `stakeWei()` and refuses to send when it disagrees with
the relay's quote. So a chain whose `stakeWei` differs from the registry is not
merely inconsistent — **nobody can play it**.

That means the config change and the on-chain change are ONE operation. There is
a window between them where the chain is down; keep it short.

## Procedure

1. **Confirm the current state.**
   ```
   cd coordination-app/tests/e2e && node scripts/check-stake-parity.mjs
   ```

2. **Change the registry and the workflow together** — `stake_base_units`,
   `stake_usd_cents`, `peg_native_usd_cents`, `max_tranche_base_units`, and the
   matching `XCHAIN_*` literals. `cargo test -p chain-registry` must pass; it
   compares the two and checks the declared USD against the peg price.

3. **Run `setConfig` on each chain**, via CI (never a local broadcast — the
   standards forbid manual production changes):
   ```
   COORDINATION_GAME=<address> NEW_STAKE_WEI=<wei> \
     forge script script/SetStake.s.sol --rpc-url <chain> --broadcast
   ```
   The script reads the live config and passes every other field through
   unchanged, prints a before/after, and asserts the value actually moved — a
   broadcast returning success is not proof.

4. **Re-run the parity check.** It must print PASS for every chain.

5. **Update the `coordination-app` copies** (frontend display strings, e2e
   harness `stakeWei`) in the same session, or they silently disagree.

## Choosing the number

Two constraints, and only one of them binds:

- **Gas must feel small.** Measured: a full game is ~350k gas. On Base that is
  $0.004 at current gas — 0.1% of a $5 stake. On Ethereum L1 it is $0.04 at
  0.064 gwei but **$3.25 at 5 gwei and $13 at 20 gwei**, i.e. comparable to or
  several times the whole stake. L1 is the binding constraint, and no stake
  price fixes it.
- **Nobody should use the game to freeze an ETH/SOL rate.** This does NOT bind.
  The FX option and the cost of playing both scale linearly with the stake, so
  the ratio is stake-invariant (~2% at every size). What actually bounds it is
  the 40-minute exposure window (`MATCH_DURATION_SECS` 1800 +
  `QUOTE_MAX_AGE_SECS` 600) and `FX_BAND_BPS` 1500. Lengthening matches would
  reopen it; changing the price would not.

Current anchor: **$5.00**, one shared value across all EVM chains. Base and
Ethereum are both ETH-denominated, so a single `stakeWei` keeps them equal in
USD forever with no oracle — which is what went wrong before, when three chains
were pegged independently at ETH $1,562 / $3,000 / $1,600.
