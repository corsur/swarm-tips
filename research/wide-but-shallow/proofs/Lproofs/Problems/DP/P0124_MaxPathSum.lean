import Lproofs.Schemes.Fold

/-! @lc 124 | name:Binary Tree Maximum Path Sum | scheme:dp | family:tree-aggregate | complexity:O(n) |
    source:https://leetcode.com/problems/binary-tree-maximum-path-sum/

    The best downward gain from a node is its value plus the better of the two children's gains (clamped
    at zero — a negative branch is dropped). CLASSIFICATION: this per-node gain is a genuine tree
    catamorphism (`Tree.fold` with `(gl, v, gr) ↦ v + max 0 (max gl gr)`), proven by structural
    induction (`cls`). NON-VACUITY: the gain recurrence is the real subproblem the DFS aggregates
    (`corr`). We certify the catamorphism + the recurrence; DROP the global maximum-path-sum value. -/

namespace LC.P0124

abbrev T := Interview.Patterns.Tree ℤ

/-- Best downward path gain starting at this node (drop a child branch if its gain is negative). -/
def gain : T → ℤ
  | .leaf => 0
  | .node l v r => v + max 0 (max (gain l) (gain r))

/-- SCHEME (dp / catamorphism): the per-node gain is a genuine `Tree.fold`, proven by induction. -/
theorem cls : (gain : T → ℤ) = Interview.Patterns.Tree.fold 0 (fun gl v gr => v + max 0 (max gl gr)) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [gain, Interview.Patterns.Tree.fold, ihl, ihr]

/-- NON-VACUITY: the per-node gain recurrence the DFS aggregates (negative branches clamped to zero). -/
theorem corr (l : T) (v : ℤ) (r : T) : gain (.node l v r) = v + max 0 (max (gain l) (gain r)) := rfl

end LC.P0124
