import Lproofs.Schemes.Fold

/-! @lc 199 | name:Binary Tree Right Side View | scheme:dp | family:tree-aggregate | complexity:O(n) |
    source:https://leetcode.com/problems/binary-tree-right-side-view/

    The right-side view lists, per depth, the rightmost node visible. CLASSIFICATION: a genuine tree
    catamorphism — at a node, prepend its value, then below it the right subtree's view dominates and the
    left subtree's view shows through only where the right is shallower:
    `rv (node l v r) = v :: (rv r ++ (rv l).drop (rv r).length)` (`cls`, a `Tree.fold`). NON-VACUITY: we
    prove the view has exactly one entry per level — its length equals the tree height (`corr`) — pinning
    the catamorphism to real per-level work. -/

namespace LC.P0199

abbrev T := Interview.Patterns.Tree ℤ

/-- Per-level rightmost values: prepend the node, right subtree dominates, left fills deeper-only levels. -/
def rv : T → List ℤ
  | .leaf => []
  | .node l v r => v :: (rv r ++ (rv l).drop (rv r).length)

/-- Tree height. -/
def height : T → ℕ
  | .leaf => 0
  | .node l _ r => 1 + max (height l) (height r)

/-- SCHEME (dp / catamorphism): the right-view is a genuine `Tree.fold`, proven by induction. -/
theorem cls :
    (rv : T → List ℤ)
      = Interview.Patterns.Tree.fold [] (fun lv v rv => v :: (rv ++ lv.drop rv.length)) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [rv, Interview.Patterns.Tree.fold, ihl, ihr]

/-- NON-VACUITY: the right-side view has exactly one entry per level — its length is the tree height. -/
theorem corr : ∀ t : T, (rv t).length = height t
  | .leaf => rfl
  | .node l v r => by
    simp only [rv, height, List.length_cons, List.length_append, List.length_drop]
    rw [corr l, corr r]
    omega

end LC.P0199
