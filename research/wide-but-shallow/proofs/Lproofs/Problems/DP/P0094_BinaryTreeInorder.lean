import Lproofs.Schemes.Fold

/-! @lc 94 | name:Binary Tree Inorder Traversal | scheme:dp | family:tree-aggregate | complexity:O(n) |
    source:https://leetcode.com/problems/binary-tree-sol-traversal/

    Return the in-order traversal. CLASSIFICATION: the traversal is a genuine tree catamorphism
    (`Tree.fold` with combiner `(l, v, r) ↦ l ++ v :: r`), proven by structural induction (`cls`).
    NON-VACUITY: the node recurrence — left subtree, then root, then right subtree — is the real
    visiting order (`corr`). We certify the catamorphism + the recurrence. -/

namespace LC.P0094

abbrev T := Interview.Patterns.Tree ℤ

def sol : T → List ℤ
  | .leaf => []
  | .node l v r => sol l ++ v :: sol r

/-- SCHEME (dp / catamorphism): in-order is a `Tree.fold`, proven by induction. -/
theorem cls : (sol : T → List ℤ) = Interview.Patterns.Tree.fold [] (fun l v r => l ++ v :: r) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [sol, Interview.Patterns.Tree.fold, ihl, ihr]

/-- NON-VACUITY: the node recurrence — left, root, right — the genuine visiting order. -/
theorem corr (l : T) (v : ℤ) (r : T) : sol (.node l v r) = sol l ++ v :: sol r := rfl


/-- GROUND INSTANCE (official example 1): tree [1,null,2,3] visits in-order as [1,3,2]. -/
def exT : T := .node .leaf 1 (.node (.node .leaf 3 .leaf) 2 .leaf)

theorem vec : sol exT = [1, 3, 2] := by decide

end LC.P0094
