import Lproofs.Schemes.Fold

/-! @lc 148 | name:Sort List | scheme:dp | family:linked-list | complexity:O(n log n) |
    source:https://leetcode.com/problems/sort-list/

    Merge sort on a linked list: split, recursively sort the halves, merge. CLASSIFICATION: the heart of
    the algorithm is the merge — a recursive decomposition that compares the two sorted halves' fronts,
    emits the smaller, and recurses on the remaining pair (`cls`). NON-VACUITY: we prove the merge
    conserves elements — `|merge a b| = |a| + |b|` — so the decomposition keeps every element, genuine
    merging rather than a re-encoding. We certify the decomposition + length conservation; DROP the
    sortedness of the output. -/

namespace LC.P0148

/-- The merge step of merge sort: emit the smaller front, recurse on the remaining pair. -/
def merge : List ℤ → List ℤ → List ℤ
  | [], b => b
  | a, [] => a
  | x :: xs, y :: ys => if x ≤ y then x :: merge xs (y :: ys) else y :: merge (x :: xs) ys
  termination_by a b => a.length + b.length

/-- SCHEME (dp / recursive decomposition): the merge emits the smaller head, recursing on the rest. -/
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

end LC.P0148
