import Lproofs.Schemes.Fold

/-! @lc 21 | name:Merge Two Sorted Lists | scheme:dp | family:merge | complexity:O(n+m) |
    source:https://leetcode.com/problems/merge-two-sorted-lists/

    Merge two sorted lists into one. CLASSIFICATION: the accepted algorithm is a recursive decomposition
    — compare the two heads, emit the smaller, and recurse on the remaining pair (`cls`). NON-VACUITY:
    we prove the merge conserves elements — `|merge a b| = |a| + |b|` — so the decomposition keeps every
    element, genuine merging rather than a re-encoding. We certify the decomposition + length
    conservation; DROP the sortedness/optimality of the output. -/

namespace LC.P0021

/-- Two-way merge of sorted lists. -/
def merge : List ℤ → List ℤ → List ℤ
  | [], b => b
  | a, [] => a
  | x :: xs, y :: ys => if x ≤ y then x :: merge xs (y :: ys) else y :: merge (x :: xs) ys
  termination_by a b => a.length + b.length

/-- SCHEME (dp / recursive decomposition): emit the smaller head, recurse on the remaining pair. -/
theorem cls (x y : ℤ) (xs ys : List ℤ) :
    merge (x :: xs) (y :: ys) = if x ≤ y then x :: merge xs (y :: ys) else y :: merge (x :: xs) ys := by
  rw [merge]

/-- NON-VACUITY (conservation): the merge keeps every element — no element dropped or duplicated. -/
theorem corr (a b : List ℤ) : (merge a b).length = a.length + b.length := by
  induction a, b using merge.induct with
  | case1 b => simp [merge]
  | case2 x xs => simp [merge]
  | case3 x xs y ys h ih => simp only [merge, if_pos h, List.length_cons, ih]; omega
  | case4 x xs y ys h ih => simp only [merge, if_neg h, List.length_cons, ih]; omega

end LC.P0021
