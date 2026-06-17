import Lproofs.Patterns

/-! @lc 108 | name:Convert Sorted Array to Binary Search Tree | scheme:dp | family:tree-construct |
    complexity:O(n) | source:https://leetcode.com/problems/convert-sorted-array-to-binary-search-tree/

    Build a BST from a sorted array by taking the middle element as root and recursing on the two
    halves (a divide-and-conquer / recursive decomposition). Correctness: the in-order traversal of
    the built tree recovers the original array — i.e. it is a valid BST holding exactly those
    elements in order. (Height-balance also holds by the midpoint split; not formalized here.) -/

namespace LC.P0108

/-- In-order traversal. -/
def inorder : Interview.Patterns.Tree ℤ → List ℤ
  | .leaf => []
  | .node l v r => inorder l ++ v :: inorder r

/-- Build a balanced BST from a list; `fuel` keeps the recursion structural (use `length`). -/
def build : ℕ → List ℤ → Interview.Patterns.Tree ℤ
  | 0, _ => .leaf
  | _ + 1, [] => .leaf
  | fuel + 1, x :: rest =>
    let xs := x :: rest
    let m := xs.length / 2
    .node (build fuel (xs.take m)) (xs.getD m 0) (build fuel (xs.drop (m + 1)))

/-- The construction returned to the caller. -/
def sol (xs : List ℤ) : Interview.Patterns.Tree ℤ := build xs.length xs

/-- SCHEME (dp / divide-and-conquer): the construction is a recursive decomposition — a nonempty
    array becomes a node whose value is the midpoint and whose subtrees are built recursively from
    the left and right halves. This is exactly `sol`'s engine `build` (cf. P0427's quad-split). -/
theorem cls (fuel : ℕ) (x : ℤ) (rest : List ℤ) :
    build (fuel + 1) (x :: rest) =
      .node (build fuel ((x :: rest).take ((x :: rest).length / 2)))
        ((x :: rest).getD ((x :: rest).length / 2) 0)
        (build fuel ((x :: rest).drop ((x :: rest).length / 2 + 1))) :=
  rfl

/-- In-order traversal is itself a tree catamorphism (a fold over the tree). -/
theorem inorder_fold : inorder = Interview.Patterns.Tree.fold [] (fun l v r => l ++ v :: r) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [inorder, Interview.Patterns.Tree.fold, ihl, ihr]

/-- With enough fuel, the in-order traversal of the built tree is the original list. -/
theorem inorder_build : ∀ fuel (xs : List ℤ), xs.length ≤ fuel → inorder (build fuel xs) = xs := by
  intro fuel
  induction fuel with
  | zero =>
    intro xs h
    simp only [Nat.le_zero, List.length_eq_zero_iff] at h
    simp [h, build, inorder]
  | succ fuel ih =>
    intro xs h
    match xs with
    | [] => simp [build, inorder]
    | x :: rest =>
      simp only [List.length_cons] at h
      have hpos : 0 < (x :: rest).length := by simp
      have hm : (x :: rest).length / 2 < (x :: rest).length := Nat.div_lt_self hpos (by norm_num)
      have hbt : ((x :: rest).take ((x :: rest).length / 2)).length ≤ fuel := by
        rw [List.length_take, List.length_cons]; omega
      have hbd : ((x :: rest).drop ((x :: rest).length / 2 + 1)).length ≤ fuel := by
        rw [List.length_drop, List.length_cons]; omega
      simp only [build, inorder]
      rw [ih _ hbt, ih _ hbd, List.getD_eq_getElem _ _ hm, ← List.drop_eq_getElem_cons hm,
        List.take_append_drop]

/-- CORRECT (round-trip): the in-order traversal of the built BST is exactly the input list. -/
theorem corr (xs : List ℤ) : inorder (sol xs) = xs :=
  inorder_build xs.length xs (le_refl _)

end LC.P0108
