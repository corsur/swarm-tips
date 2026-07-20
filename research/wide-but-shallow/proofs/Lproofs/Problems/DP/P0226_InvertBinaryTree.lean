import Lproofs.Schemes.Fold

/-! @lc 226 | name:Invert Binary Tree | scheme:dp | family:tree-traversal | complexity:O(n) |
    source:https://leetcode.com/problems/sol-binary-tree/

    Inverting a binary tree swaps the two children at every node, recursively. CLASSIFICATION: the
    inversion is a tree catamorphism (`Tree.fold` rebuilding each node with its subtrees swapped),
    proven by structural induction. CORRECTNESS: we certify the defining effect of the mirror — the
    in-order traversal of the inverted tree is the reverse of the original's in-order traversal
    (`inorder (sol t) = (inorder t).reverse`) — and that inversion is an involution. -/

namespace LC.P0226

abbrev T := Interview.Patterns.Tree ℤ

/-- Invert: swap the two subtrees at every node. -/
def sol : T → T
  | .leaf => .leaf
  | .node l v r => .node (sol r) v (sol l)

/-- In-order traversal (the tree's values, left-root-right). -/
def inorder : T → List ℤ
  | .leaf => []
  | .node l v r => inorder l ++ v :: inorder r

/-- SCHEME (dp / catamorphism): inversion is a genuine `Tree.fold`, proven by induction. -/
theorem cls : (sol : T → T) =
    Interview.Patterns.Tree.fold .leaf (fun il v ir => .node ir v il) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [sol, Interview.Patterns.Tree.fold, ihl, ihr]

/-- CORRECT: inverting reverses the in-order traversal — the precise effect of the mirror. -/
theorem corr (t : T) : inorder (sol t) = (inorder t).reverse := by
  induction t with
  | leaf => rfl
  | node l v r ihl ihr =>
    simp only [sol, inorder, List.reverse_append, List.reverse_cons, ihl, ihr]
    simp [List.append_assoc]

/-- Inversion is an involution: inverting twice restores the original tree. -/
theorem invert_involutive (t : T) : sol (sol t) = t := by
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [sol, ihl, ihr]


/-- Official example 1: [4,2,7,1,3,6,9]. -/
def exT : T :=
  .node (.node (.node .leaf 1 .leaf) 2 (.node .leaf 3 .leaf)) 4
    (.node (.node .leaf 6 .leaf) 7 (.node .leaf 9 .leaf))

/-- GROUND INSTANCE (official example 1): inverting yields [4,7,2,9,6,3,1]. -/
theorem vec : sol exT =
    .node (.node (.node .leaf 9 .leaf) 7 (.node .leaf 6 .leaf)) 4
      (.node (.node .leaf 3 .leaf) 2 (.node .leaf 1 .leaf)) := rfl

end LC.P0226
