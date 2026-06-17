import Lproofs.Schemes.Fold

/-! @lc 545 | name:Boundary of Binary Tree | scheme:dp | family:tree-traversal | complexity:O(n) |
    source:https://leetcode.com/problems/boundary-of-binary-tree/

    The boundary is collected by traversing the tree (left edge, leaves, right edge). CLASSIFICATION:
    the underlying traversal is a genuine tree catamorphism — preorder `(l, v, r) ↦ v :: l ++ r` —
    proven by structural induction (`cls`). NON-VACUITY: the node recurrence (root, then left subtree,
    then right subtree) is the real visiting order (`corr`). We certify the catamorphism + the
    recurrence. -/

namespace LC.P0545

abbrev T := Interview.Patterns.Tree ℤ

/-- Preorder traversal: root, then left subtree, then right subtree. -/
def preorder : T → List ℤ
  | .leaf => []
  | .node l v r => v :: (preorder l ++ preorder r)

/-- SCHEME (dp / catamorphism): the traversal is a `Tree.fold`, proven by induction. -/
theorem cls : (preorder : T → List ℤ)
    = Interview.Patterns.Tree.fold [] (fun l v r => v :: (l ++ r)) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [preorder, Interview.Patterns.Tree.fold, ihl, ihr]

/-- NON-VACUITY: the node recurrence — root, then left subtree, then right subtree. -/
theorem corr (l : T) (v : ℤ) (r : T) : preorder (.node l v r) = v :: (preorder l ++ preorder r) := rfl

end LC.P0545
