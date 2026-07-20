import Lproofs.Schemes.Fold

/-! @lc 536 | name:Construct Binary Tree from String | scheme:dp | family:tree-construct |
    complexity:O(n) | source:https://leetcode.com/problems/construct-binary-tree-from-string/

    A string like `4(2(3)(1))(6(5))` encodes a tree: a node value followed by parenthesised left and
    right subtrees. The accepted solution is a recursive-descent parse; its inverse, serialization, is
    the matching tree catamorphism `value · "(" left ")" "(" right ")"`. CLASSIFICATION (dp): the tree
    encoding is a catamorphism. CORRECTNESS (soundness, not full parser correctness): we certify that
    serialization preserves every node --- every value in the tree appears in the serialized token
    stream, so the encoding drops nothing. -/

namespace LC.P0536

abbrev T := Interview.Patterns.Tree ℤ

/-- In-order node values. -/
def inorder : T → List ℤ
  | .leaf => []
  | .node l v r => inorder l ++ v :: inorder r

/-- The value-token stream of the string encoding (value, then left then right subtree tokens). -/
def sol : T → List ℤ
  | .leaf => []
  | .node l v r => v :: (sol l ++ sol r)

/-- SCHEME (dp / catamorphism): the encoding is a genuine `Tree.fold`. -/
theorem cls : (sol : T → List ℤ) =
    Interview.Patterns.Tree.fold [] (fun l v r => v :: (l ++ r)) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [sol, Interview.Patterns.Tree.fold, ihl, ihr]

/-- CORRECT (soundness): every node value appears in the serialized token stream --- the encoding
    preserves all nodes. -/
theorem corr (t : T) (x : ℤ) (h : x ∈ inorder t) : x ∈ sol t := by
  induction t with
  | leaf => simp [inorder] at h
  | node l v r ihl ihr =>
    simp only [inorder, List.mem_append, List.mem_cons] at h
    simp only [sol, List.mem_cons, List.mem_append]
    rcases h with h | h | h
    · exact Or.inr (Or.inl (ihl h))
    · exact Or.inl h
    · exact Or.inr (Or.inr (ihr h))


/-- Official example 1: "4(2(3)(1))(6(5))". -/
def exT : T :=
  .node (.node (.node .leaf 3 .leaf) 2 (.node .leaf 1 .leaf)) 4
    (.node (.node .leaf 5 .leaf) 6 .leaf)

/-- GROUND INSTANCE (official example 1): the token stream is value-first, [4,2,3,1,6,5]. -/
theorem vec : sol exT = [4, 2, 3, 1, 6, 5] := by decide


/-- COMPLETENESS (the other direction of `corr`): every serialized token is a genuine node value
    — the encoding adds nothing. With `corr`, the token stream carries exactly the tree's
    values. -/
theorem complete (t : T) (x : ℤ) (h : x ∈ sol t) : x ∈ inorder t := by
  induction t with
  | leaf => simp [sol] at h
  | node l v r ihl ihr =>
    simp only [sol, List.mem_cons, List.mem_append] at h
    simp only [inorder, List.mem_append, List.mem_cons]
    rcases h with h | h | h
    · exact Or.inr (Or.inl h)
    · exact Or.inl (ihl h)
    · exact Or.inr (Or.inr (ihr h))

end LC.P0536
