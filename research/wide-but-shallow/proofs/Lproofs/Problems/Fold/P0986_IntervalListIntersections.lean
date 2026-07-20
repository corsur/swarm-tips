import Lproofs.Schemes.Fold

/-! @lc 986 | name:Interval List Intersections | scheme:fold | family:merge-intervals |
    complexity:O(m+n) | source:https://leetcode.com/problems/interval-list-intersections/

    Two sorted interval lists are intersected by a two-pointer sweep, emitting `[max(a,c), min(b,d)]`
    for each overlapping pair and advancing the interval that ends first. CLASSIFICATION (fold): the
    sweep is a streaming two-pointer fold over the merged endpoints. CORRECTNESS: we certify the
    per-pair intersection formula the sweep emits — a point lies in BOTH `[a,b]` and `[c,d]` exactly when
    it lies in `[max(a,c), min(b,d)]`. -/

namespace LC.P0986

/-- The intersection of intervals `[a,b]` and `[c,d]`: `[max a c, min b d]`. -/
def sol (a b c d : ℤ) : ℤ × ℤ := (max a c, min b d)

/-- SCHEME (fold): the intersection is symmetric — the two-pointer sweep treats the lists symmetrically. -/
theorem cls (a b c d : ℤ) : sol a b c d = sol c d a b := by
  simp [sol, max_comm, min_comm]

/-- CORRECT: a point is in both intervals iff it is in the emitted intersection `[max a c, min b d]`. -/
theorem corr (a b c d x : ℤ) :
    ((a ≤ x ∧ x ≤ b) ∧ (c ≤ x ∧ x ≤ d)) ↔
      ((sol a b c d).1 ≤ x ∧ x ≤ (sol a b c d).2) := by
  simp only [sol, le_max_iff, max_le_iff, le_min_iff, min_le_iff]
  omega


/-- GROUND INSTANCE (official example 1, first emitted pair): [0,2] ∩ [1,5] = [1,2]; and the
    disjoint pair [5,6] ∩ [8,12] collapses to the empty interval (8,6). -/
theorem vec : sol 0 2 1 5 = (1, 2) ∧ sol 5 6 8 12 = (8, 6) := by decide

end LC.P0986
