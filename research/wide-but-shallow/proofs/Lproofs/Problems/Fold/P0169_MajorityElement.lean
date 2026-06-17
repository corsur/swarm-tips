import Lproofs.Schemes.Fold

/-! @lc 169 | name:Majority Element | scheme:fold | family:hashing | complexity:O(n) |
    source:https://leetcode.com/problems/majority-element/ -/

namespace LC.P0169
open Interview.Patterns

/-- Spec: a strict majority element — appears in more than half the positions. -/
def spec (a : List ℕ) (x : ℕ) : Prop := 2 * a.count x > a.length

/-- Accepted O(n) frequency solution: build the frequency multiset with a streaming fold
    (`charCount`), then return the first element whose frequency exceeds half. The fold-built
    `charCount` is the solution's actual working structure (Boyer–Moore is the same fold with a
    tally accumulator). -/
def sol (a : List ℕ) : Option ℕ := a.find? (fun x => decide (2 * (charCount a).count x > a.length))

/-- SCHEME (fold): the frequency structure `sol` reads is the streaming `charCount` fold. -/
theorem cls : IsFold (fun s : List ℕ => s.foldl (fun m c => insert c m) (0 : Multiset ℕ)) :=
  fold_charCount

/-- CORRECT: whenever the search returns an element, it is a strict majority. -/
theorem corr (a : List ℕ) {x : ℕ} (h : sol a = some x) : spec a x := by
  simp only [sol] at h
  have hp := List.find?_some h
  rw [decide_eq_true_iff, charCount_eq, Multiset.coe_count] at hp
  exact hp

end LC.P0169
