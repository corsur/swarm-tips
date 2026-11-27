-- @lean-policy: 4
import SwarmProofs.Chains.ProtocolMath.Defs.V1

/-- Multi-step deposit & recoupment trace folding `compute_route_and_recoup` (extracted directly
    from `crates/protocol-math/src/lib.rs`) across an arbitrary sequence of incoming deposits. -/
def runRecoupTrace (rent_floor advance recouped : UInt64) :
    List UInt64 → Option (Nat × Nat × UInt64)
  | [] => some (0, 0, recouped)
  | d :: ds =>
    match u64CheckedAdd rent_floor d with
    | none => none
    | some bal =>
      match compute_route_and_recoup bal rent_floor advance recouped with
      | none => none
      | some sp =>
        match runRecoupTrace rent_floor advance sp.new_recouped ds with
        | none => none
        | some (tb, tr, finalRec) =>
          some (sp.to_backer.toNat + tb, sp.to_recipient.toNat + tr, finalRec)

/-- Exact panic-freedom, lamport conservation, backer cap & rent-floor safety of
    `compute_route_and_recoup` across all `UInt64` inputs up to `u64::MAX`. -/
def RouteAndRecoupSpec : Prop :=
  ∀ (balance rent_floor advance recouped : UInt64),
    (recouped.toNat ≤ advance.toNat ↔
      ∃ sp, compute_route_and_recoup balance rent_floor advance recouped = some sp) ∧
    (∀ sp, compute_route_and_recoup balance rent_floor advance recouped = some sp →
      sp.to_backer.toNat + sp.to_recipient.toNat + sp.remaining_balance.toNat = balance.toNat ∧
      sp.new_recouped.toNat = recouped.toNat + sp.to_backer.toNat ∧
      sp.new_recouped.toNat ≤ advance.toNat ∧
      (rent_floor.toNat ≤ balance.toNat → sp.remaining_balance = rent_floor) ∧
      (balance.toNat ≤ rent_floor.toNat →
        sp.to_backer = 0 ∧ sp.to_recipient = 0 ∧ sp.remaining_balance = balance))

/-- Exact full-recoupment rejection & equivalence of `compute_mark_default` to
    `compute_route_and_recoup` whenever `recouped < advance`. -/
def MarkDefaultSpec : Prop :=
  ∀ (balance rent_floor advance recouped : UInt64),
    (recouped.toNat < advance.toNat ↔
      ∃ sp, compute_mark_default balance rent_floor advance recouped = some sp) ∧
    (recouped.toNat < advance.toNat →
      compute_mark_default balance rent_floor advance recouped =
        compute_route_and_recoup balance rent_floor advance recouped)

/-- Multi-step deposit split-invariance (anti-front-run batch equivalence): splitting a deposit
    into any sequence `ds : List UInt64` yields the exact same cumulative `(backer, recipient, recouped)`. -/
def MultiStepRecoupSplitSpec : Prop :=
  ∀ (rent_floor advance recouped : UInt64) (ds : List UInt64),
    recouped.toNat ≤ advance.toNat →
    rent_floor.toNat + ds.foldl (fun acc d => acc + d.toNat) 0 < U64_MOD →
    ∃ spBatch,
      compute_route_and_recoup
        (UInt64.ofNat (rent_floor.toNat + ds.foldl (fun acc d => acc + d.toNat) 0))
        rent_floor advance recouped = some spBatch ∧
      runRecoupTrace rent_floor advance recouped ds =
        some (spBatch.to_backer.toNat, spBatch.to_recipient.toNat, spBatch.new_recouped)

/-- Track II [Rung 1/3 → Verified Rust Settlement Core (`crates/protocol-math/src/lib.rs`)]:
    Proves full `UInt64` panic-freedom, exact lamport conservation, backer recoupment ceiling,
    rent-floor protection, `compute_mark_default` equivalence, and multi-step deposit split-invariance
    (anti-front-run batch equivalence) directly against the mechanically AST-extracted Rust functions
    `compute_route_and_recoup` and `compute_mark_default` across ALL `UInt64` inputs up to `u64::MAX`.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  RouteAndRecoupSpec ∧ MarkDefaultSpec ∧ MultiStepRecoupSplitSpec
