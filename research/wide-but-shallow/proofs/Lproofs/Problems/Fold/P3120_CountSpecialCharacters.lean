import Lproofs.Schemes.Fold

/-! @lc 3120 | name:Count the Number of Special Characters I | scheme:fold | family:string-other |
    complexity:O(n) | source:https://leetcode.com/problems/count-the-number-of-special-characters-i/ -/

namespace LC.P3120
open Interview.Patterns

/-- Spec: a "special" letter appears in the string in both its lowercase and uppercase forms. -/
def spec (s : List Char) (c : Char) : Prop := c.isLower = true ∧ c ∈ s ∧ c.toUpper ∈ s

/-- Editorial set solution: the distinct lowercase letters whose uppercase also occurs
    (membership built by a streaming fold). -/
def sol (s : List Char) : List Char :=
  s.dedup.filter (fun c => c.isLower && decide (c.toUpper ∈ s))

/-- SCHEME (fold): the seen-set is a streaming fold. -/
theorem cls : IsFold (fun xs : List Char => xs.foldl (fun st x => insert x st) (∅ : Finset Char)) :=
  ⟨fun st x => insert x st, ∅, fun _ => rfl⟩

/-- CORRECT: every reported character is special. -/
theorem corr (s : List Char) {c : Char} (h : c ∈ sol s) : spec s c := by
  simp only [sol, List.mem_filter, List.mem_dedup, Bool.and_eq_true, decide_eq_true_eq] at h
  exact ⟨h.2.1, h.1, h.2.2⟩

end LC.P3120
