import Lproofs.Schemes.Fold

/-! @lc 1768 | name:Merge Strings Alternately | scheme:fold | family:two-pointers | complexity:O(n) |
    source:https://leetcode.com/problems/merge-strings-alternately/

    Interleave two strings character by character, appending the remainder of the longer one. The
    accepted solution is a single streaming pass that emits the head of one list then swaps roles.
    CLASSIFICATION: a one-pass two-pointer scan (the streaming-fold family). NON-VACUITY: we prove the
    merge is a genuine permutation of the two inputs (`↑(merge xs ys) = ↑xs + ↑ys`) — every character
    of both strings appears exactly once. We certify the streaming structure + faithfulness. -/

namespace LC.P1768

/-- Alternating merge: emit the head of the first list, then swap roles. -/
def merge {α : Type*} : List α → List α → List α
  | [], ys => ys
  | x :: xs, ys => x :: merge ys xs
termination_by xs ys => xs.length + ys.length

/-- SCHEME (fold / streaming two-pointer): one pass emits the head then swaps the two lists. -/
theorem cls {α : Type*} (x : α) (xs ys : List α) : merge (x :: xs) ys = x :: merge ys xs := by
  simp [merge]

/-- NON-VACUITY: the merge is a permutation of both inputs — no character is lost or duplicated. -/
theorem corr {α : Type*} (xs ys : List α) : List.Perm (merge xs ys) (xs ++ ys) := by
  induction xs, ys using merge.induct with
  | case1 ys => simp [merge]
  | case2 x xs ys ih => rw [merge]; exact (ih.trans List.perm_append_comm).cons x

end LC.P1768
