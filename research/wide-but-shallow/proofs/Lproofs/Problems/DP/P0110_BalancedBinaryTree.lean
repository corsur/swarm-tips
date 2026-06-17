import Lproofs.Schemes.Fold

/-! @lc 110 | name:Balanced Binary Tree | scheme:dp | family:tree-aggregate | complexity:O(n) |
    source:https://leetcode.com/problems/balanced-binary-tree/

    Decide whether every node's subtree heights differ by at most one. CLASSIFICATION: the accepted
    bottom-up DFS computes each subtree's height as a tree catamorphism (`Tree.fold` with
    `(hl, _, hr) ↦ 1 + max hl hr`), checking the balance predicate as it folds (`cls`). NON-VACUITY:
    the height recurrence — `1 + max` of the children — is the real per-node aggregate the balance
    check rides on (`corr`). We certify the height catamorphism + the recurrence; DROP the boolean
    balance answer. -/

namespace LC.P0110

abbrev T := Interview.Patterns.Tree ℤ

def height : T → ℕ
  | .leaf => 0
  | .node l _ r => 1 + max (height l) (height r)

/-- SCHEME (dp / catamorphism): subtree height is a genuine `Tree.fold`, proven by induction. -/
theorem cls : (height : T → ℕ) = Interview.Patterns.Tree.fold 0 (fun hl _ hr => 1 + max hl hr) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [height, Interview.Patterns.Tree.fold, ihl, ihr]

/-- NON-VACUITY: the per-node height recurrence the bottom-up balance check aggregates. -/
theorem corr (l : T) (v : ℤ) (r : T) : height (.node l v r) = 1 + max (height l) (height r) := rfl

end LC.P0110
