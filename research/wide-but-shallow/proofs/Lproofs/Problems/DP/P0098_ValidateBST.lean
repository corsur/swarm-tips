import Lproofs.Schemes.Fold

/-! @lc 98 | name:Validate Binary Search Tree | scheme:dp | family:bst | complexity:O(n) |
    source:https://leetcode.com/problems/validate-binary-search-tree/

    A binary tree is a valid BST iff its in-order traversal is strictly increasing — which is exactly
    what the accepted in-order check tests (`List.Pairwise (· < ·)`, equivalently the consecutive
    strictly-increasing check, for the strict order `<`). CLASSIFICATION: the in-order traversal is a
    genuine tree catamorphism, proven by structural induction. CORRECTNESS: we prove the accepted check
    is equivalent to the GENUINE bounded-BST property `isBST` (every left value `<` node `<` every right
    value), by structural induction — not a restatement of the check. -/

namespace LC.P0098

abbrev T := Interview.Patterns.Tree ℤ

/-- In-order traversal. -/
def inorder : T → List ℤ
  | .leaf => []
  | .node l v r => inorder l ++ v :: inorder r

/-- The accepted check: valid ⟺ the in-order traversal is strictly increasing. -/
def sol (t : T) : Prop := List.Pairwise (· < ·) (inorder t)

/-- The genuine (bounded) BST property: at every node, all left values `<` `v` `<` all right values. -/
def isBST : T → Prop
  | .leaf => True
  | .node l v r => isBST l ∧ isBST r ∧ (∀ y ∈ inorder l, y < v) ∧ (∀ y ∈ inorder r, v < y)

/-- SCHEME (dp / catamorphism): the in-order traversal is a tree catamorphism — `Tree.fold` with the
    combiner `(left, v, right) ↦ left ++ v :: right`, proven by induction over the tree. -/
theorem cls : (inorder : T → List ℤ) = Interview.Patterns.Tree.fold [] (fun l v r => l ++ v :: r) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [inorder, Interview.Patterns.Tree.fold, ihl, ihr]

/-- CORRECT: the accepted in-order-sorted check accepts `t` iff `t` is a genuine BST. -/
theorem corr (t : T) : sol t ↔ isBST t := by
  induction t with
  | leaf => simp [sol, isBST, inorder]
  | node l v r ihl ihr =>
    simp only [sol, inorder, isBST] at *
    rw [List.pairwise_append, List.pairwise_cons]
    constructor
    · rintro ⟨hpl, ⟨hvr, hpr⟩, hcross⟩
      exact ⟨ihl.mp hpl, ihr.mp hpr, fun y hy => hcross y hy v List.mem_cons_self, hvr⟩
    · rintro ⟨hbl, hbr, hlv, hvr⟩
      refine ⟨ihl.mpr hbl, ⟨hvr, ihr.mpr hbr⟩, ?_⟩
      intro a ha b hb
      rcases List.mem_cons.mp hb with rfl | hb
      · exact hlv a ha
      · exact lt_trans (hlv a ha) (hvr b hb)

end LC.P0098
