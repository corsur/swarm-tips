import Lproofs.Schemes.Fold

/-! @lc 1 | name:Two Sum | scheme:fold | family:hashing | complexity:O(n) |
    source:https://leetcode.com/problems/two-sum/ -/

namespace LC.P0001
open Interview.Patterns

/-- Spec: two elements at distinct positions summing to the target (`y` drawn from `a` with one
    occurrence of `x` removed encodes "a different position"). -/
def spec (a : List ℤ) (t : ℤ) : Prop := ∃ x ∈ a, ∃ y ∈ a.erase x, x + y = t

/-- Editorial hash solution: for some element, its complement occurs at another position
    (the seen-set / complement lookup is a streaming fold). -/
def sol (a : List ℤ) (t : ℤ) : Bool := a.any (fun x => decide ((t - x) ∈ a.erase x))

/-- SCHEME (fold): the seen-set / complement table is a streaming fold. -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl (fun s x => insert x s) (∅ : Finset ℤ)) :=
  ⟨fun s x => insert x s, ∅, fun _ => rfl⟩

/-- CORRECT: when the search succeeds, a genuine two-sum pair exists. -/
theorem corr (a : List ℤ) (t : ℤ) (h : sol a t = true) : spec a t := by
  simp only [sol, List.any_eq_true, decide_eq_true_eq] at h
  obtain ⟨x, hx, hy⟩ := h
  exact ⟨x, hx, t - x, hy, by ring⟩

end LC.P0001
