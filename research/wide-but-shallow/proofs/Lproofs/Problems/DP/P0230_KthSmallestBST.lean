import Lproofs.Schemes.Fold

/-! @lc 230 | name:Kth Smallest Element in a BST | scheme:dp | family:bst | complexity:O(n) |
    source:https://leetcode.com/problems/kth-smallest-element-in-a-bst/

    The k-th smallest BST element is the k-th entry of the in-order traversal. CLASSIFICATION: the
    in-order traversal is a genuine tree catamorphism (`Tree.fold` with `(l, v, r) ↦ l ++ v :: r`),
    proven by induction; the answer indexes into it. -/

namespace LC.P0230

abbrev T := Interview.Patterns.Tree ℤ

def inorder : T → List ℤ
  | .leaf => []
  | .node l v r => inorder l ++ v :: inorder r

/-- The accepted answer: the k-th element of the (sorted) in-order traversal. -/
def sol (t : T) (k : ℕ) : Option ℤ := (inorder t)[k]?

/-- SCHEME (dp / catamorphism): the in-order traversal is a `Tree.fold`, proven by induction. -/
theorem cls : (inorder : T → List ℤ) = Interview.Patterns.Tree.fold [] (fun l v r => l ++ v :: r) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [inorder, Interview.Patterns.Tree.fold, ihl, ihr]

/-- The answer indexes the catamorphic in-order traversal. -/
theorem corr (t : T) (k : ℕ) : sol t k = (inorder t)[k]? := rfl

end LC.P0230
