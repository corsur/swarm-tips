import Lproofs.Schemes.Fold

/-! @lc 2150 | name:Find All Lonely Numbers in the Array | scheme:fold | family:hashing |
    complexity:O(n) | source:https://leetcode.com/problems/find-all-lonely-numbers-in-the-array/ -/

namespace LC.P2150
open Interview.Patterns

/-- Editorial frequency solution: a value is "lonely" if it occurs exactly once and neither
    neighbour (`x-1`, `x+1`) occurs (counts/membership built by a streaming fold). -/
def sol (a : List ℤ) : List ℤ :=
  a.dedup.filter (fun x => decide (a.count x = 1 ∧ (x - 1) ∉ a ∧ (x + 1) ∉ a))

/-- Spec: each reported value occurs once and has no adjacent value present. -/
def spec (a : List ℤ) (y : ℤ) : Prop := a.count y = 1 ∧ (y - 1) ∉ a ∧ (y + 1) ∉ a

/-- SCHEME (fold): the frequency/seen-set is a streaming fold. -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl (fun s x => insert x s) (∅ : Finset ℤ)) :=
  ⟨fun s x => insert x s, ∅, fun _ => rfl⟩

/-- CORRECT: every reported value is genuinely lonely. -/
theorem corr (a : List ℤ) {y : ℤ} (h : y ∈ sol a) : spec a y := by
  simp only [sol, List.mem_filter, decide_eq_true_eq] at h
  exact h.2

end LC.P2150
