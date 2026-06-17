import Lproofs.Schemes.Fold

/-! @lc 173 | name:Binary Search Tree Iterator | scheme:dp | family:bst | complexity:O(n) |
    source:https://leetcode.com/problems/binary-search-tree-iterator/

    The iterator returns the BST's keys in ascending (in-order) order. CLASSIFICATION: the in-order
    traversal it streams is a genuine tree catamorphism — `Tree.fold` with combiner
    `(left, v, right) ↦ left ++ v :: right`, proven by structural induction (not assumed). -/

namespace LC.P0173

abbrev T := Interview.Patterns.Tree ℤ

/-- In-order traversal — the sequence the iterator yields. -/
def inorder : T → List ℤ
  | .leaf => []
  | .node l v r => inorder l ++ v :: inorder r

def sol (t : T) : List ℤ := inorder t

/-- SCHEME (dp / catamorphism): the in-order stream is a tree catamorphism, proven by induction. -/
theorem cls : (inorder : T → List ℤ) = Interview.Patterns.Tree.fold [] (fun l v r => l ++ v :: r) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [inorder, Interview.Patterns.Tree.fold, ihl, ihr]

/-- The iterator's output is exactly the catamorphic in-order traversal. -/
theorem corr (t : T) : sol t = inorder t := rfl

end LC.P0173
