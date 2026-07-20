import Lproofs.Schemes.Fold

/-! @lc 109 | name:Convert Sorted List to Binary Search Tree | scheme:dp | family:tree-construct |
    complexity:O(n) | source:https://leetcode.com/problems/convert-sorted-list-to-binary-search-tree/

    A height-balanced BST is built from a sorted list by taking the middle element as the root and
    recursing on the two halves. CLASSIFICATION (dp): a divide-and-conquer recursion splitting the list
    at its midpoint. CORRECTNESS: we certify the defining BST property of the construction — its in-order
    traversal recovers exactly the original sorted list, so the tree stores precisely those values in
    order. -/

namespace LC.P0109

abbrev T := Interview.Patterns.Tree ℤ

/-- In-order traversal. -/
def inorder : T → List ℤ
  | .leaf => []
  | .node l v r => inorder l ++ v :: inorder r

/-- Build a balanced BST from the list, splitting at the midpoint (`fuel` bounds the recursion). -/
def sol : ℕ → List ℤ → T
  | 0, _ => .leaf
  | f + 1, l =>
    match l.drop (l.length / 2) with
    | [] => .leaf
    | mid :: rest => .node (sol f (l.take (l.length / 2))) mid (sol f rest)

/-- SCHEME (dp / recursive decomposition): `sol` splits the list at its midpoint and recurses on
    the two halves; the in-order reading `corr` checks it against is a genuine `Tree.fold`. -/
theorem cls : (∀ (f : ℕ) (l : List ℤ), sol (f + 1) l =
      match l.drop (l.length / 2) with
      | [] => .leaf
      | mid :: rest => .node (sol f (l.take (l.length / 2))) mid (sol f rest)) ∧
    (inorder : T → List ℤ) = Interview.Patterns.Tree.fold [] (fun l v r => l ++ v :: r) := by
  refine ⟨fun f l => rfl, ?_⟩
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [inorder, Interview.Patterns.Tree.fold, ihl, ihr]

/-- CORRECT: the in-order traversal of the built tree is the original sorted list. -/
theorem corr (f : ℕ) (l : List ℤ) (hf : l.length ≤ f) : inorder (sol f l) = l := by
  induction f generalizing l with
  | zero =>
    have : l = [] := List.length_eq_zero_iff.mp (Nat.le_zero.mp hf)
    subst this; rfl
  | succ f ih =>
    rw [sol]
    cases hl : l.drop (l.length / 2) with
    | nil =>
      have hlen : (l.drop (l.length / 2)).length = 0 := by rw [hl]; rfl
      rw [List.length_drop] at hlen
      have : l = [] := List.length_eq_zero_iff.mp (by omega)
      subst this; rfl
    | cons mid rest =>
      have hmid : mid :: rest = l.drop (l.length / 2) := hl.symm
      have hrlen : rest.length = l.length - (l.length / 2) - 1 := by
        have := congrArg List.length hmid
        rw [List.length_drop, List.length_cons] at this
        omega
      have htlen : (l.take (l.length / 2)).length ≤ f := by
        rw [List.length_take]; omega
      have hrlen' : rest.length ≤ f := by omega
      simp only [inorder, ih _ htlen, ih _ hrlen']
      rw [hmid, List.take_append_drop]


/-- GROUND INSTANCE (official example 1): building from [-10,-3,0,5,9] yields a tree whose
    in-order reading is the original sorted list — the height-balance round-trip on the judge's
    input. -/
theorem vec : inorder (sol 5 [-10, -3, 0, 5, 9]) = [-10, -3, 0, 5, 9] := by decide

end LC.P0109
