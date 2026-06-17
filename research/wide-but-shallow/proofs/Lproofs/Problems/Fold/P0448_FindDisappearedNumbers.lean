import Lproofs.Schemes.Fold

/-! @lc 448 | name:Find All Numbers Disappeared in an Array | scheme:fold | family:hashing |
    complexity:O(n) | source:https://leetcode.com/problems/find-all-numbers-disappeared-in-an-array/ -/

namespace LC.P0448
open Interview.Patterns

/-- Editorial O(n) membership solution: keep the values of `1..n` absent from the array
    (membership built by a streaming fold). -/
def sol (a : List ℕ) : List ℕ := (List.range (a.length + 1)).filter (fun x => decide (1 ≤ x ∧ x ∉ a))

/-- Spec: each reported value lies in `[1, n]` and is absent from the array. -/
def spec (a : List ℕ) (y : ℕ) : Prop := 1 ≤ y ∧ y ≤ a.length ∧ y ∉ a

/-- SCHEME (fold): the membership/seen-set is a streaming fold. -/
theorem cls : IsFold (fun xs : List ℕ => xs.foldl (fun s x => insert x s) (∅ : Finset ℕ)) :=
  fold_seenSet

/-- CORRECT: every reported value is in range and genuinely absent from the array. -/
theorem corr (a : List ℕ) {y : ℕ} (h : y ∈ sol a) : spec a y := by
  simp only [sol, List.mem_filter, List.mem_range] at h
  obtain ⟨hlt, hp⟩ := h
  have hd := of_decide_eq_true hp
  exact ⟨hd.1, by omega, hd.2⟩

end LC.P0448
