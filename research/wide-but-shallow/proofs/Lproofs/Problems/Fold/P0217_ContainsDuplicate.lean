import Lproofs.Schemes.Fold

/-! @lc 217 | name:Contains Duplicate | scheme:fold | family:hashing | complexity:O(n) |
    source:https://leetcode.com/problems/contains-duplicate/ -/

namespace LC.P0217
open Interview.Patterns

/-- Spec: the array contains a duplicate iff it is not nodup. -/
def spec (xs : List ℕ) : Prop := ¬ xs.Nodup

/-- Editorial hash-set solution: a duplicate exists iff the set of seen values is smaller than
    the array (the seen-set is built by a fold). -/
def sol (xs : List ℕ) : Bool := decide (xs.toFinset.card < xs.length)

/-- SCHEME: the seen-set is a streaming fold (accumulator: a finite set). -/
theorem cls : IsFold (fun xs : List ℕ => xs.foldl (fun s x => insert x s) (∅ : Finset ℕ)) :=
  fold_seenSet

/-- CORRECT: the strict card-vs-length test holds iff the array has a duplicate. -/
theorem corr (xs : List ℕ) : sol xs = true ↔ spec xs := by
  rw [sol, spec, decide_eq_true_eq, List.card_toFinset, ← List.dedup_eq_self]
  have hsub := List.dedup_sublist xs
  constructor
  · intro h he; rw [he] at h; exact lt_irrefl _ h
  · intro h
    rcases lt_or_eq_of_le hsub.length_le with hlt | heq
    · exact hlt
    · exact absurd (hsub.eq_of_length heq) h
