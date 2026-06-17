import Lproofs.Schemes.Fold

/-! @lc 41 | name:First Missing Positive | scheme:fold | family:hashing | complexity:O(n) |
    source:https://leetcode.com/problems/first-missing-positive/ -/

namespace LC.P0041
open Interview.Patterns

/-- Spec: the answer is a positive integer absent from the array. (The smallest such lies in
    `[1, n+1]`, so scanning that range returns it.) -/
def spec (a : List ℕ) (x : ℕ) : Prop := 1 ≤ x ∧ x ∉ a

/-- The array's membership set, built by a streaming seen-set fold (`= a.toFinset`). -/
def seen (a : List ℕ) : Finset ℕ := a.foldl (fun s x => insert x s) (∅ : Finset ℕ)

/-- Editorial O(n) membership solution: build the membership set with a streaming fold, then scan
    `0..n+1` for the first positive value absent from it. The fold-built `seen a` is the structure
    the in-place index editorial emulates. -/
def sol (a : List ℕ) : Option ℕ :=
  (List.range (a.length + 2)).find? (fun x => decide (1 ≤ x ∧ x ∉ seen a))

/-- SCHEME (fold): the membership set `sol` reads is the streaming seen-set fold. -/
theorem cls : IsFold (fun xs : List ℕ => xs.foldl (fun s x => insert x s) (∅ : Finset ℕ)) :=
  fold_seenSet

/-- CORRECT: whenever the search returns a value, it is positive and absent from the array. -/
theorem corr (a : List ℕ) {x : ℕ} (h : sol a = some x) : spec a x := by
  simp only [sol] at h
  have hp := List.find?_some h
  rw [decide_eq_true_iff] at hp
  simpa only [spec, seen, seenSet_eq_toFinset, List.mem_toFinset] using hp

end LC.P0041
