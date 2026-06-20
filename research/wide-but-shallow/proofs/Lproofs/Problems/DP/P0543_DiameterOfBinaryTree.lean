import Lproofs.Schemes.Fold

/-! @lc 543 | name:Diameter of Binary Tree | scheme:dp | family:tree-aggregate | complexity:O(n) |
    source:https://leetcode.com/problems/diameter-of-binary-tree/

    The diameter is the longest path (in edges) between any two nodes; the one-pass DFS returns, per
    node, both its height and the best diameter seen, combining `height(left) + height(right)` at each
    node. CLASSIFICATION: a tree catamorphism (`depth` is `Tree.fold`). CORRECTNESS: we certify the
    genuine bound the algorithm's structure guarantees — the diameter is at most twice the height
    (`diam t ≤ 2 * depth t`), proven by structural induction. -/

namespace LC.P0543

abbrev T := Interview.Patterns.Tree ℤ

/-- Height: `1 + max` of children heights. -/
def depth : T → ℕ
  | .leaf => 0
  | .node l _ r => 1 + max (depth l) (depth r)

/-- Diameter (in edges): best of the children's diameters and the through-root path `hₗ + hᵣ`. -/
def diam : T → ℕ
  | .leaf => 0
  | .node l _ r => max (max (diam l) (diam r)) (depth l + depth r)

/-- SCHEME (dp / catamorphism): the height aggregate is a genuine `Tree.fold`. -/
theorem cls : (depth : T → ℕ) =
    Interview.Patterns.Tree.fold 0 (fun dl _ dr => 1 + max dl dr) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [depth, Interview.Patterns.Tree.fold, ihl, ihr]

/-- CORRECT: the diameter is at most twice the height — the longest path cannot exceed two root-to-leaf
    descents. A genuine inductive bound, not the bare recurrence. -/
theorem corr (t : T) : diam t ≤ 2 * depth t := by
  induction t with
  | leaf => simp [diam, depth]
  | node l v r ihl ihr => simp only [diam, depth] <;> omega

end LC.P0543
