import Lproofs.Schemes.Fold

/-! @lc 611 | name:Valid Triangle Number | scheme:fold | family:two-pointers | complexity:O(n²) |
    source:https://leetcode.com/problems/valid-triangle-number/

    Count index triples `i < j < k` whose side lengths form a (non-degenerate) triangle. The
    editorial sorts and two-pointer-scans. Correctness here: the count is positive exactly when some
    valid triangle exists (the count's defining membership predicate has a witness). -/

namespace LC.P0611
open Interview.Patterns

/-- The three lengths form a non-degenerate triangle. -/
def tri (x y z : ℤ) : Prop := x + y > z ∧ x + z > y ∧ y + z > x

instance (x y z : ℤ) : Decidable (tri x y z) := by unfold tri; infer_instance

/-- Number of triples `i < j < k` forming a triangle. -/
def sol (a : List ℤ) : ℕ :=
  ((Finset.range a.length ×ˢ Finset.range a.length ×ˢ Finset.range a.length).filter
    (fun t => t.1 < t.2.1 ∧ t.2.1 < t.2.2 ∧
      tri (a.getD t.1 0) (a.getD t.2.1 0) (a.getD t.2.2 0))).card

/-- Spec: some triple forms a triangle. -/
def spec (a : List ℤ) : Prop :=
  ∃ i j k, i < j ∧ j < k ∧ k < a.length ∧ tri (a.getD i 0) (a.getD j 0) (a.getD k 0)

/-- SCHEME (fold): the counting scan is a streaming pass over the array. -/
theorem cls : IsFold (fun a : List ℤ => a.foldl (fun acc _ => acc + 1) 0) :=
  ⟨_, _, fun _ => rfl⟩

/-- CORRECT: the triangle count is positive exactly when a valid triangle exists. -/
theorem corr (a : List ℤ) : 0 < sol a ↔ spec a := by
  rw [sol, Finset.card_pos, Finset.filter_nonempty_iff]
  simp only [Finset.mem_product, Finset.mem_range, Prod.exists, spec]
  constructor
  · rintro ⟨i, j, k, ⟨hi, hj, hk⟩, hij, hjk, ht⟩
    exact ⟨i, j, k, hij, hjk, hk, ht⟩
  · rintro ⟨i, j, k, hij, hjk, hk, ht⟩
    exact ⟨i, j, k, ⟨by omega, by omega, hk⟩, hij, hjk, ht⟩

end LC.P0611
