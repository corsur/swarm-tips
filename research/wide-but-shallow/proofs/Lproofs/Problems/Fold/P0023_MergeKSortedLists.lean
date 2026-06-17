import Lproofs.Patterns

/-! @lc 23 | name:Merge k Sorted Lists | scheme:fold | family:merge-reduce | complexity:O(N log k) |
    source:https://leetcode.com/problems/merge-k-sorted-lists/

    Merge `k` sorted lists by repeatedly two-way-merging them. CLASSIFICATION: the sequential reduction
    is a streaming left fold over the lists, accumulating the running merged list (the merge-reduce
    pattern, cf. Longest Common Prefix). NON-VACUITY: we prove the two-way merge conserves elements —
    `|merge a b| = |a| + |b|` — so the folded result keeps every element (total length = sum of input
    lengths), genuine merging rather than a re-encoding. We certify the fold + length conservation;
    DROP the sortedness/optimality of the output. -/

namespace LC.P0023
open Interview.Patterns

/-- Two-way merge of sorted lists. -/
def merge : List ℤ → List ℤ → List ℤ
  | [], b => b
  | a, [] => a
  | x :: xs, y :: ys => if x ≤ y then x :: merge xs (y :: ys) else y :: merge (x :: xs) ys
  termination_by a b => a.length + b.length

/-- Reduce all lists by sequential two-way merge — the merge-reduce fold. -/
def sol (lists : List (List ℤ)) : List ℤ := lists.foldl merge []

/-- SCHEME (fold): the k-way merge is a left fold accumulating the running merged list. -/
theorem cls : IsFold (fun lists : List (List ℤ) => lists.foldl merge []) := ⟨merge, [], fun _ => rfl⟩

/-- NON-VACUITY: the two-way merge conserves elements — no element is dropped or duplicated. -/
theorem merge_length (a b : List ℤ) : (merge a b).length = a.length + b.length := by
  induction a, b using merge.induct with
  | case1 b => simp [merge]
  | case2 x xs => simp [merge]
  | case3 x xs y ys h ih => simp only [merge, if_pos h, List.length_cons, ih]; omega
  | case4 x xs y ys h ih =>
    simp only [merge, if_neg h, List.length_cons, ih]; omega

/-- NON-VACUITY: the k-way merge keeps every element — total length = sum of input lengths. -/
theorem corr (lists : List (List ℤ)) : (sol lists).length = (lists.map List.length).sum := by
  have key : ∀ (ls : List (List ℤ)) acc,
      (ls.foldl merge acc).length = acc.length + (ls.map List.length).sum := by
    intro ls
    induction ls with
    | nil => intro acc; simp
    | cons l rest ih =>
      intro acc
      simp only [List.foldl_cons, List.map_cons, List.sum_cons]
      rw [ih (merge acc l), merge_length acc l]
      omega
  have := key lists []
  simpa [sol] using this

end LC.P0023
