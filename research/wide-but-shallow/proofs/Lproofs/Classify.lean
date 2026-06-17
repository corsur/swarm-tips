import Lproofs.Generators

/-!
# Formal classification of the algebraic tier (proof of concept)

A *classification certificate* is a machine-checked proof that a problem's canonical
(accepted-style) solution inhabits the defining scheme of an algebraic generator. For that
tier this grounds the family label in a theorem rather than a human rating.

**Scope and honesty.**
* Covers only the algebraic generators: recursive-fold / semiring DP, running-state /
  monoid-group scan, relaxation / least fixpoint. The periphery (greedy, hash lookup,
  two-pointer exchange, binary search, simulation, design) has *no* formal membership
  predicate and cannot be classified this way.
* A certificate shows the labeled method *correctly and characteristically* solves the
  problem; it does not prove the label is the uniquely-simplest accepted family, nor does
  it disambiguate problems solvable by several families. Inter-rater reliability remains the
  appropriate check for those judgments.
* This is a proof of concept on three archetypes (one per algebraic generator), not a
  classification of all 769 problems (which would require formalizing each accepted
  solution -- future work).
-/

namespace Interview.Classify
open Interview

/-! ## Recursive-fold generator: Climbing Stairs (LC 70)
The accepted O(1)-memory iterative solution is the linear DP recurrence -- a fold. -/

/-- DP specification: ways to climb `n` stairs taking 1 or 2 at a time. -/
def stairs : ℕ → ℕ
  | 0 => 1
  | 1 => 1
  | (n + 2) => stairs (n + 1) + stairs n

/-- The accepted iterative solution: carry the last two values (the O(1) fold). -/
def stairsIter : ℕ → ℕ × ℕ
  | 0 => (1, 1)
  | (n + 1) => ((stairsIter n).2, (stairsIter n).1 + (stairsIter n).2)

theorem stairsIter_eq (n : ℕ) : stairsIter n = (stairs n, stairs (n + 1)) := by
  induction n with
  | zero => rfl
  | succ k ih => rw [stairsIter, ih]; simp [stairs, Nat.add_comm]

/-- CLASSIFICATION: the iterative accepted solution computes the DP recurrence, so
    Climbing Stairs is in the recursive-fold (linear-DP / semiring) generator. -/
theorem classify_climbingStairs (n : ℕ) : (stairsIter n).1 = stairs n := by
  rw [stairsIter_eq]

/-! ## Running-state generator: Range Sum Query -- Immutable (LC 303)
The prefix-array range query is a group scan -- certified by `range_fold`. -/

/-- CLASSIFICATION: the prefix-difference range query *is* a group scan. -/
theorem classify_rangeSumQuery {G : Type*} [Group G] (a : List G) {i j : ℕ} (h : i ≤ j) :
    (pre a i)⁻¹ * pre a j = ((a.drop i).take (j - i)).prod :=
  range_fold a h

/-! ## Relaxation generator: reachability (e.g. LC 1971, Find if Path Exists)
The reachable-set solution is a Bellman least fixpoint -- certified by `reach_is_dp_fixpoint`. -/

/-- CLASSIFICATION: graph reachability *is* a least-fixpoint dynamic program. -/
theorem classify_reachability {V : Type*} (s : Set V) (r : V → V → Prop) :
    reachOp s r (OrderHom.lfp (reachOp s r)) = OrderHom.lfp (reachOp s r) :=
  reach_is_dp_fixpoint s r

end Interview.Classify
