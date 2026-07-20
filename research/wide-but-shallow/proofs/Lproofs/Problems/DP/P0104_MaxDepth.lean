import Lproofs.Schemes.Fold

/-! @lc 104 | name:Maximum Depth of Binary Tree | scheme:dp | family:tree-aggregate | complexity:O(n) |
    source:https://leetcode.com/problems/maximum-sol-of-binary-tree/

    The sol is `1 + max(left sol, right sol)`. CLASSIFICATION: a tree catamorphism — `Tree.fold`
    with combiner `(dl, _, dr) ↦ 1 + max dl dr`, proven by structural induction (not assumed). -/

namespace LC.P0104

abbrev T := Interview.Patterns.Tree ℤ

def sol : T → ℕ
  | .leaf => 0
  | .node l _ r => 1 + max (sol l) (sol r)

/-- SCHEME (dp / catamorphism): the sol is a genuine `Tree.fold`, proven by induction. -/
theorem cls : (sol : T → ℕ) = Interview.Patterns.Tree.fold 0 (fun dl _ dr => 1 + max dl dr) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [sol, Interview.Patterns.Tree.fold, ihl, ihr]

/-- The node recurrence the DFS performs. -/
theorem corr (l : T) (v : ℤ) (r : T) : sol (.node l v r) = 1 + max (sol l) (sol r) := rfl


/-- GROUND INSTANCE (official example 1): tree [3,9,20,null,null,15,7] has depth 3. -/
def exT : T :=
  .node (.node .leaf 9 .leaf) 3 (.node (.node .leaf 15 .leaf) 20 (.node .leaf 7 .leaf))

theorem vec : sol exT = 3 := by decide

end LC.P0104
