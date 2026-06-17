import Lproofs.Schemes.Fold

/-! @lc 219 | name:Contains Duplicate II | scheme:fold | family:hashing | complexity:O(n) |
    source:https://leetcode.com/problems/contains-duplicate-ii/ -/

namespace LC.P0219
open Interview.Patterns

/-- Spec: two equal values occur at distinct indices within distance `k`. -/
def spec (a : List ℤ) (k : ℕ) : Prop :=
  ∃ i j, i < j ∧ j < a.length ∧ a.getD i 0 = a.getD j 0 ∧ j - i ≤ k

/-- Editorial sliding-window hash set: a repeat within the last `k` indices. -/
def sol (a : List ℤ) (k : ℕ) : Bool :=
  decide (∃ i ∈ Finset.range a.length, ∃ j ∈ Finset.range a.length,
    i < j ∧ a.getD i 0 = a.getD j 0 ∧ j - i ≤ k)

/-- SCHEME (fold): the windowed seen-set is a streaming fold. -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl (fun s x => insert x s) (∅ : Finset ℤ)) :=
  ⟨fun s x => insert x s, ∅, fun _ => rfl⟩

/-- CORRECT: the boolean answer matches the existence of a close-by duplicate. -/
theorem corr (a : List ℤ) (k : ℕ) : sol a k = true ↔ spec a k := by
  simp only [sol, decide_eq_true_eq, Finset.mem_range]
  constructor
  · rintro ⟨i, _, j, hj, hij, heq, hk⟩; exact ⟨i, j, hij, hj, heq, hk⟩
  · rintro ⟨i, j, hij, hj, heq, hk⟩; exact ⟨i, by omega, j, hj, hij, heq, hk⟩

end LC.P0219
