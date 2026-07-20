import Lproofs.Schemes.Fold

/-! @lc 88 | name:Merge Sorted Array | scheme:dp | family:merge | complexity:O(n+m) |
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


/-- The merge keeps exactly the elements of both inputs — a permutation, strengthening `corr`'s
    length conservation. -/
theorem merge_perm : ∀ (a b : List ℤ), (sol a b).Perm (a ++ b) := by
  intro a b
  induction a, b using sol.induct with
  | case1 b => simp [sol]
  | case2 x xs => simp [sol]
  | case3 x xs y ys h ih =>
    rw [show sol (x :: xs) (y :: ys) = x :: sol xs (y :: ys) from by simp [sol, if_pos h],
      List.cons_append]
    exact ih.cons x
  | case4 x xs y ys h ih =>
    rw [show sol (x :: xs) (y :: ys) = y :: sol (x :: xs) ys from by simp [sol, if_neg h]]
    exact (ih.cons y).trans List.perm_middle.symm

/-- FULL CORRECTNESS: merging sorted inputs yields a sorted output. With `merge_perm`, this is
    the complete functional specification of the two-way merge: the result is exactly the two
    inputs' elements, in order. -/
theorem merge_sorted : ∀ (a b : List ℤ),
    a.Pairwise (· ≤ ·) → b.Pairwise (· ≤ ·) → (sol a b).Pairwise (· ≤ ·) := by
  intro a b
  induction a, b using sol.induct with
  | case1 b => intro _ hb; simpa [sol] using hb
  | case2 x xs => intro ha _; simpa [sol] using ha
  | case3 x xs y ys h ih =>
    intro ha hb
    rw [show sol (x :: xs) (y :: ys) = x :: sol xs (y :: ys) from by simp [sol, if_pos h],
      List.pairwise_cons]
    obtain ⟨hxall, hxs⟩ := List.pairwise_cons.mp ha
    obtain ⟨hyall, hys⟩ := List.pairwise_cons.mp hb
    refine ⟨fun b hb' => ?_, ih hxs hb⟩
    rcases List.mem_append.mp ((merge_perm xs (y :: ys)).mem_iff.mp hb') with hx | hy'
    · exact hxall b hx
    · rcases List.mem_cons.mp hy' with rfl | hys'
      · exact h
      · exact le_trans h (hyall b hys')
  | case4 x xs y ys h ih =>
    intro ha hb
    rw [show sol (x :: xs) (y :: ys) = y :: sol (x :: xs) ys from by simp [sol, if_neg h],
      List.pairwise_cons]
    obtain ⟨hxall, hxs⟩ := List.pairwise_cons.mp ha
    obtain ⟨hyall, hys⟩ := List.pairwise_cons.mp hb
    have hyx : y ≤ x := by omega
    refine ⟨fun b hb' => ?_, ih ha hys⟩
    rcases List.mem_append.mp ((merge_perm (x :: xs) ys).mem_iff.mp hb') with hx' | hys'
    · rcases List.mem_cons.mp hx' with rfl | hxs'
      · exact hyx
      · exact le_trans hyx (hxall b hxs')
    · exact hyall b hys'

end LC.P0088
