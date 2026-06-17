import Lproofs.Schemes.Fold

/-! @lc 104 | name:Maximum Depth of Binary Tree | scheme:dp | family:tree-aggregate | complexity:O(n) |
    source:https://leetcode.com/problems/maximum-depth-of-binary-tree/

    The depth is `1 + max(left depth, right depth)`. CLASSIFICATION: a tree catamorphism — `Tree.fold`
    with combiner `(dl, _, dr) ↦ 1 + max dl dr`, proven by structural induction (not assumed). -/

namespace LC.P0104

abbrev T := Interview.Patterns.Tree ℤ

def depth : T → ℕ
  | .leaf => 0
  | .node l _ r => 1 + max (depth l) (depth r)

/-- SCHEME (dp / catamorphism): the depth is a genuine `Tree.fold`, proven by induction. -/
theorem cls : (depth : T → ℕ) = Interview.Patterns.Tree.fold 0 (fun dl _ dr => 1 + max dl dr) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [depth, Interview.Patterns.Tree.fold, ihl, ihr]

/-- The node recurrence the DFS performs. -/
theorem corr (l : T) (v : ℤ) (r : T) : depth (.node l v r) = 1 + max (depth l) (depth r) := rfl

end LC.P0104
