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

/-- Triangles whose largest index is `k`: pairs `i < j < k` completing a triangle with entry `k`. -/
def countAt (a : List ℤ) (k : ℕ) : ℕ :=
  ((Finset.range k ×ˢ Finset.range k).filter
    (fun p => p.1 < p.2 ∧ tri (a.getD p.1 0) (a.getD p.2 0) (a.getD k 0))).card

theorem sol_eq_sum (a : List ℤ) : sol a = ∑ k ∈ Finset.range a.length, countAt a k := by
  have hmem : ∀ x ∈ (Finset.range a.length ×ˢ Finset.range a.length ×ˢ
      Finset.range a.length).filter
      (fun t => t.1 < t.2.1 ∧ t.2.1 < t.2.2 ∧
        tri (a.getD t.1 0) (a.getD t.2.1 0) (a.getD t.2.2 0)),
      (fun t : ℕ × ℕ × ℕ => t.2.2) x ∈ Finset.range a.length := by
    intro x hx
    simp only [Finset.mem_filter, Finset.mem_product, Finset.mem_range] at hx
    exact Finset.mem_range.mpr hx.1.2.2
  unfold sol countAt
  rw [Finset.card_eq_sum_card_fiberwise hmem]
  refine Finset.sum_congr rfl fun k hk => ?_
  refine Finset.card_bij' (fun t _ => (t.1, t.2.1)) (fun p _ => (p.1, p.2, k)) ?_ ?_ ?_ ?_
  · intro t ht
    simp only [Finset.mem_filter, Finset.mem_product, Finset.mem_range] at ht ⊢
    obtain ⟨⟨⟨hi, hj, hm⟩, hij, hjk, htri⟩, hfib⟩ := ht
    subst hfib
    exact ⟨⟨by omega, by omega⟩, hij, htri⟩
  · intro p hp
    simp only [Finset.mem_filter, Finset.mem_product, Finset.mem_range] at hp ⊢
    obtain ⟨⟨hik, hjk⟩, hij, htri⟩ := hp
    have hk' := Finset.mem_range.mp hk
    exact ⟨⟨⟨by omega, by omega, hk'⟩, hij, hjk, htri⟩, trivial⟩
  · intro t ht
    simp only [Finset.mem_filter] at ht
    obtain ⟨x, y, z⟩ := t
    have hz : z = k := ht.2
    subst hz
    rfl
  · intro p _
    rfl

theorem foldl_range_sum (f : ℕ → ℕ) (n : ℕ) :
    (List.range n).foldl (fun acc k => acc + f k) 0 = ∑ k ∈ Finset.range n, f k := by
  induction n with
  | zero => rfl
  | succ n ih => rw [List.range_succ, List.foldl_append, ih, Finset.sum_range_succ]; rfl

/-- SCHEME (fold): `sol` is a single streaming pass over the indices — each step adds the
    triangles completed by the current entry as largest side (the count the sorted two-pointer
    scan accumulates). -/
theorem cls (a : List ℤ) :
    IsFold (fun ks : List ℕ => ks.foldl (fun acc k => acc + countAt a k) 0) ∧
    sol a = (List.range a.length).foldl (fun acc k => acc + countAt a k) 0 :=
  ⟨⟨_, _, fun _ => rfl⟩, by rw [foldl_range_sum, sol_eq_sum]⟩

/-- CORRECT: the triangle count is positive exactly when a valid triangle exists. -/
theorem corr (a : List ℤ) : 0 < sol a ↔ spec a := by
  rw [sol, Finset.card_pos, Finset.filter_nonempty_iff]
  simp only [Finset.mem_product, Finset.mem_range, Prod.exists, spec]
  constructor
  · rintro ⟨i, j, k, ⟨hi, hj, hk⟩, hij, hjk, ht⟩
    exact ⟨i, j, k, hij, hjk, hk, ht⟩
  · rintro ⟨i, j, k, hij, hjk, hk, ht⟩
    exact ⟨i, j, k, ⟨by omega, by omega, hk⟩, hij, hjk, ht⟩


/-- GROUND INSTANCE (official example 1): nums [2,2,3,4] admits exactly 3 valid triangles. -/
theorem vec : sol [2, 2, 3, 4] = 3 := by decide

end LC.P0611
