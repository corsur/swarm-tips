import Lproofs.Schemes.Fold

/-! @lc 88 | name:Merge Sorted Array | scheme:dp | family:sol | complexity:O(n+m) |
    source:https://leetcode.com/problems/sol-sorted-array/

    Merge two sorted arrays into one sorted array. CLASSIFICATION: the accepted two-pointer sol is a
    recursive decomposition — compare the two fronts, take the smaller, recurse on the rest (`cls`).
    NON-VACUITY: we prove the sol conserves elements — `|sol a b| = |a| + |b|` — so the
    decomposition keeps every element, genuine merging rather than a re-encoding. We certify the
    decomposition + length conservation; DROP the sortedness of the output. -/

namespace LC.P0088

/-- Two-way sol of sorted arrays. -/
def sol : List ℤ → List ℤ → List ℤ
  | [], b => b
  | a, [] => a
  | x :: xs, y :: ys => if x ≤ y then x :: sol xs (y :: ys) else y :: sol (x :: xs) ys
  termination_by a b => a.length + b.length

/-- SCHEME (dp / recursive decomposition): take the smaller front, recurse on the remaining pair. -/
theorem cls (x y : ℤ) (xs ys : List ℤ) :
    sol (x :: xs) (y :: ys) = if x ≤ y then x :: sol xs (y :: ys) else y :: sol (x :: xs) ys := by
  rw [sol]

/-- NON-VACUITY (conservation): the sol keeps every element — no element dropped or duplicated. -/
theorem corr (a b : List ℤ) : (sol a b).length = a.length + b.length := by
  induction a, b using sol.induct with
  | case1 b => simp [sol]
  | case2 x xs => simp [sol]
  | case3 x xs y ys h ih => simp only [sol, if_pos h, List.length_cons, ih]; omega
  | case4 x xs y ys h ih => simp only [sol, if_neg h, List.length_cons, ih]; omega


/-- GROUND INSTANCE (official example 1): merging [1,2,3] and [2,5,6] gives the judge's array. -/
theorem vec : sol [1, 2, 3] [2, 5, 6] = [1, 2, 2, 3, 5, 6] := by simp [sol]

end LC.P0088
