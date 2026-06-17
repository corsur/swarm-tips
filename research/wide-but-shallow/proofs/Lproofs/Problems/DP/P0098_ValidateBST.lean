import Lproofs.Schemes.Fold

/-! @lc 98 | name:Validate Binary Search Tree | scheme:dp | family:bst | complexity:O(n) |
    source:https://leetcode.com/problems/validate-binary-search-tree/

    A binary tree is a valid BST iff its in-order traversal is strictly increasing — which is exactly
    what the accepted in-order check tests. CLASSIFICATION: the in-order traversal is a genuine tree
    catamorphism (a fold over the tree), proven by structural induction (not assumed); the validity
    reads off it. -/

namespace LC.P0098

abbrev T := Interview.Patterns.Tree ℤ

/-- In-order traversal. -/
def inorder : T → List ℤ
  | .leaf => []
  | .node l v r => inorder l ++ v :: inorder r

/-- Valid BST ⟺ the in-order traversal is strictly increasing (the accepted check). -/
def sol (t : T) : Prop := List.Chain' (· < ·) (inorder t)

/-- SCHEME (dp / catamorphism): the in-order traversal is a tree catamorphism — `Tree.fold` with the
    combiner `(left, v, right) ↦ left ++ v :: right`, proven by induction over the tree. -/
theorem cls : (inorder : T → List ℤ) = Interview.Patterns.Tree.fold [] (fun l v r => l ++ v :: r) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [inorder, Interview.Patterns.Tree.fold, ihl, ihr]

/-- The validity check reads off the catamorphic in-order traversal. -/
theorem corr (t : T) : sol t ↔ List.Chain' (· < ·) (inorder t) := Iff.rfl

end LC.P0098
