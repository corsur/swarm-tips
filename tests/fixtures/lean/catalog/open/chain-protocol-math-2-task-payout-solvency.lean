-- @lean-policy: 4
-- @depends-on: chain-protocol-math-1-advance-recoupment-safety
import SwarmProofs.Chains.ProtocolMath.Defs.V1

/-- Exact `u128` overflow-freedom and floor-division specification of `mul_div_u64`. -/
def MulDivU64ExactSpec : Prop :=
  ∀ (a b denom : UInt64),
    (denom.toNat ≠ 0 ∧ (a.toNat * b.toNat) / denom.toNat < U64_MOD ↔
      ∃ q, mul_div_u64 a b denom = some q) ∧
    (∀ q, mul_div_u64 a b denom = some q →
      q.toNat = (a.toNat * b.toNat) / denom.toNat)

/-- Total panic-freedom, exact escrow conservation, fee-rate ceiling, and below-threshold
    full refund for `compute_task_payout` across ALL `UInt64` inputs up to `u64::MAX`. -/
def TaskPayoutConservationSpec : Prop :=
  ∀ (escrow score threshold max_score fee_bps : UInt64),
    (max_score.toNat ≠ 0 ∧ score.toNat ≤ max_score.toNat ∧
        fee_bps.toNat ≤ MAX_PROTOCOL_FEE_BPS.toNat ↔
      ∃ sp, compute_task_payout escrow score threshold max_score fee_bps = some sp) ∧
    (∀ sp, compute_task_payout escrow score threshold max_score fee_bps = some sp →
      sp.payment_amount.toNat + sp.fee_amount.toNat + sp.client_refund.toNat = escrow.toNat ∧
      sp.fee_amount.toNat * BPS_DENOMINATOR.toNat ≤ escrow.toNat * fee_bps.toNat ∧
      (score.toNat < threshold.toNat →
        sp.payment_amount = 0 ∧ sp.fee_amount = 0 ∧ sp.client_refund = escrow))

/-- Simultaneous score monotonicity of `payment_amount`, `fee_amount`, and `-client_refund`
    for `compute_task_payout` (including across the `quality_threshold` boundary). -/
def TaskPayoutScoreMonotoneSpec : Prop :=
  ∀ (escrow score1 score2 threshold max_score fee_bps : UInt64) (sp1 sp2 : TaskPayoutSplit),
    score1.toNat ≤ score2.toNat →
    compute_task_payout escrow score1 threshold max_score fee_bps = some sp1 →
    compute_task_payout escrow score2 threshold max_score fee_bps = some sp2 →
    sp1.payment_amount.toNat ≤ sp2.payment_amount.toNat ∧
    sp1.fee_amount.toNat ≤ sp2.fee_amount.toNat ∧
    sp2.client_refund.toNat ≤ sp1.client_refund.toNat

/-- Track II [Rung 2/3 → Verified Rust Settlement Core (`crates/protocol-math/src/lib.rs`)]:
    Proves exact `u128` overflow-freedom, floor-division semantics, exact escrow conservation,
    fee-rate ceiling, below-threshold full refund, and simultaneous score monotonicity of
    `payment_amount`, `fee_amount`, and `-client_refund` directly against the mechanically
    AST-extracted Rust functions `mul_div_u64` and `compute_task_payout` across ALL `UInt64` inputs
    (including `escrow = u64::MAX`, `score = max_score = u64::MAX`).
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  MulDivU64ExactSpec ∧ TaskPayoutConservationSpec ∧ TaskPayoutScoreMonotoneSpec
