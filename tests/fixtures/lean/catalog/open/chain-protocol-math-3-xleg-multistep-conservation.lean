-- @lean-policy: 4
-- @depends-on: chain-protocol-math-2-task-payout-solvency
import SwarmProofs.Chains.ProtocolMath.Defs.V1

/-- Multi-game cross-chain liquidity-pool & global stake trace over an arbitrary sequence of
    paired two-leg matches `(stake, fee_bps, outcome)`, computing
    `(totalPoolReturned, totalTrancheReserved, totalExternalPayoutsAndFeesAndPrizes, totalPlayerStakes)`
    directly via the mechanically AST-extracted Rust functions `mul_div_u64` and `compute_xleg_split`. -/
def runXLegMatchTrace : List (UInt64 × UInt64 × XOutcome) → Option (Nat × Nat × Nat × Nat)
  | [] => some (0, 0, 0, 0)
  | (stake, fee_bps, outcome) :: rest =>
    match mul_div_u64 stake fee_bps BPS_DENOMINATOR with
    | none => none
    | some fee_cut =>
      match u64CheckedSub stake fee_cut with
      | none => none
      | some tranche =>
        match compute_xleg_split stake tranche fee_bps true outcome,
              compute_xleg_split stake tranche fee_bps false outcome with
        | some s1, some s2 =>
          match runXLegMatchTrace rest with
          | none => none
          | some (totPoolReturn, totTrancheReserved, totExternalOut, totStakes) =>
            some (
              totPoolReturn + s1.to_pool_reimburse.toNat + s1.tranche_released.toNat
                + s2.to_pool_reimburse.toNat + s2.tranche_released.toNat,
              totTrancheReserved + 2 * tranche.toNat,
              totExternalOut + s1.to_player.toNat + s1.to_fee.toNat + s1.to_prize.toNat
                + s2.to_player.toNat + s2.to_fee.toNat + s2.to_prize.toNat,
              totStakes + 2 * stake.toNat
            )
        | _, _ => none

/-- Exact overflow-boundary characterization and single-leg lamport conservation across all
    four `XOutcome` variants for `compute_xleg_split`. -/
def XLegSingleConservationSpec : Prop :=
  ∀ (stake tranche fee_bps : UInt64) (seat_is_p1 : Bool) (outcome : XOutcome),
    (fee_bps.toNat ≤ MAX_PROTOCOL_FEE_BPS.toNat ∧
        (((outcome = XOutcome.P1Wins ∧ seat_is_p1 = true) ∨
          (outcome = XOutcome.P2Wins ∧ seat_is_p1 = false)) →
          stake.toNat + tranche.toNat < U64_MOD) ↔
      ∃ sp, compute_xleg_split stake tranche fee_bps seat_is_p1 outcome = some sp) ∧
    (∀ sp, compute_xleg_split stake tranche fee_bps seat_is_p1 outcome = some sp →
      sp.to_player.toNat + sp.to_fee.toNat + sp.to_prize.toNat +
        sp.to_pool_reimburse.toNat + sp.tranche_released.toNat =
        stake.toNat + tranche.toNat)

/-- Multi-game cross-chain liquidity-pool solvency (zero net pool drain) and exact global stake
    accounting across arbitrary lists of settled two-leg matches. -/
def XLegMultiMatchSolvencySpec : Prop :=
  ∀ (ms : List (UInt64 × UInt64 × XOutcome)),
    (∀ m ∈ ms, m.2.1.toNat ≤ MAX_PROTOCOL_FEE_BPS.toNat ∧ 2 * m.1.toNat < U64_MOD) →
    ∃ totPool totTranche totExt totStakes,
      runXLegMatchTrace ms = some (totPool, totTranche, totExt, totStakes) ∧
      totPool = totTranche ∧
      totExt = totStakes

/-- Track II [Rung 3/3 → Verified Rust Settlement Core (`crates/protocol-math/src/lib.rs`)]:
    Proves exact overflow-boundary characterization, single-leg conservation across all four
    `XOutcome` variants, paired two-leg cross-chain liquidity-pool solvency (zero net pool drain),
    and arbitrary multi-game trace global conservation directly against the mechanically
    AST-extracted Rust function `compute_xleg_split`.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  XLegSingleConservationSpec ∧ XLegMultiMatchSolvencySpec
