import Lproofs.Schemes.Fold

/-! @lc 226 | name:Invert Binary Tree | scheme:dp | family:tree-traversal | complexity:O(n) |
    source:https://leetcode.com/problems/invert-binary-tree/

    Inverting a binary tree swaps the two children at every node, recursively. CLASSIFICATION: the
    inversion is a tree catamorphism (`Tree.fold` rebuilding each node with its subtrees swapped),
    proven by structural induction. CORRECTNESS: we certify the defining effect of the mirror — the
    in-order traversal of the inverted tree is the reverse of the original's in-order traversal
    (`inorder (invert t) = (inorder t).reverse`) — and that inversion is an involution. -/

namespace LC.P0226

abbrev T := Interview.Patterns.Tree ℤ

/-- Invert: swap the two subtrees at every node. -/
def invert : T → T
  | .leaf => .leaf
  | .node l v r => .node (invert r) v (invert l)

/-- In-order traversal (the tree's values, left-root-right). -/
def inorder : T → List ℤ
  | .leaf => []
  | .node l v r => inorder l ++ v :: inorder r

/-- SCHEME (dp / catamorphism): inversion is a genuine `Tree.fold`, proven by induction. -/
theorem cls : (invert : T → T) =
    Interview.Patterns.Tree.fold .leaf (fun il v ir => .node ir v il) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [invert, Interview.Patterns.Tree.fold, ihl, ihr]

/-- CORRECT: inverting reverses the in-order traversal — the precise effect of the mirror. -/
theorem corr (t : T) : inorder (invert t) = (inorder t).reverse := by
  induction t with
  | leaf => rfl
  | node l v r ihl ihr =>
    simp only [invert, inorder, List.reverse_append, List.reverse_cons, ihl, ihr]
    simp [List.append_assoc]

/-- Inversion is an involution: inverting twice restores the original tree. -/
theorem invert_involutive (t : T) : invert (invert t) = t := by
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [invert, ihl, ihr]

end LC.P0226
