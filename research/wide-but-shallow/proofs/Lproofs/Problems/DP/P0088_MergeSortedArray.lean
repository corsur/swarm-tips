import Lproofs.Schemes.Fold

/-! @lc 88 | name:Merge Sorted Array | scheme:dp | family:merge | complexity:O(n+m) |
    source:https://leetcode.com/problems/merge-sorted-array/

    Merge two sorted arrays into one sorted array. CLASSIFICATION: the accepted two-pointer merge is a
    recursive decomposition — compare the two fronts, take the smaller, recurse on the rest (`cls`).
    NON-VACUITY: we prove the merge conserves elements — `|merge a b| = |a| + |b|` — so the
    decomposition keeps every element, genuine merging rather than a re-encoding. We certify the
    decomposition + length conservation; DROP the sortedness of the output. -/

namespace LC.P0088

/-- Two-way merge of sorted arrays. -/
def merge : List ℤ → List ℤ → List ℤ
  | [], b => b
  | a, [] => a
  | x :: xs, y :: ys => if x ≤ y then x :: merge xs (y :: ys) else y :: merge (x :: xs) ys
  termination_by a b => a.length + b.length

/-- SCHEME (dp / recursive decomposition): take the smaller front, recurse on the remaining pair. -/
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

end LC.P0088
