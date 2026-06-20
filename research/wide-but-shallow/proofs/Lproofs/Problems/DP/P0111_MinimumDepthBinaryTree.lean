import Lproofs.Schemes.Fold

/-! @lc 111 | name:Minimum Depth of Binary Tree | scheme:dp | family:tree-aggregate | complexity:O(n) |
    source:https://leetcode.com/problems/minimum-depth-of-binary-tree/

    The minimum depth is the fewest nodes from the root to the nearest leaf, taking the one present
    child when a node has only one. CLASSIFICATION: a tree catamorphism (`depth` is `Tree.fold`).
    CORRECTNESS: we certify the genuine relationship that makes "minimum" meaningful — the minimum depth
    never exceeds the maximum depth (`minDepth t ≤ depth t`), proven by structural induction handling the
    one-child case that distinguishes this problem from `maxDepth`. -/

namespace LC.P0111

abbrev T := Interview.Patterns.Tree ℤ

/-- Maximum depth (height): `1 + max` of the children's depths. -/
def depth : T → ℕ
  | .leaf => 0
  | .node l _ r => 1 + max (depth l) (depth r)

/-- Minimum depth: shortest root-to-leaf; a one-child node must descend into the present child. -/
def minDepth : T → ℕ
  | .leaf => 0
  | .node l _ r =>
    match l, r with
    | .leaf, .leaf => 1
    | .leaf, r => 1 + minDepth r
    | l, .leaf => 1 + minDepth l
    | l, r => 1 + min (minDepth l) (minDepth r)

/-- SCHEME (dp / catamorphism): the height aggregate is a genuine `Tree.fold`. -/
theorem cls : (depth : T → ℕ) =
    Interview.Patterns.Tree.fold 0 (fun dl _ dr => 1 + max dl dr) := by
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [depth, Interview.Patterns.Tree.fold, ihl, ihr]

/-- CORRECT: the minimum depth never exceeds the maximum depth — the shortest root-to-leaf path is no
    longer than the longest. The one-child case is where `minDepth` genuinely differs from `maxDepth`. -/
theorem corr (t : T) : minDepth t ≤ depth t := by
  have h0 : depth (.leaf : T) = 0 := rfl
  induction t with
  | leaf => rfl
  | node l v r ihl ihr =>
    cases l with
    | leaf =>
      cases r with
      | leaf => rfl
      | node rl rv rr =>
        show 1 + minDepth (.node rl rv rr) ≤ 1 + max (depth (.leaf : T)) (depth (.node rl rv rr))
        omega
    | node ll lv lr =>
      cases r with
      | leaf =>
        show 1 + minDepth (.node ll lv lr) ≤ 1 + max (depth (.node ll lv lr)) (depth (.leaf : T))
        omega
      | node rl rv rr =>
        show 1 + min (minDepth (.node ll lv lr)) (minDepth (.node rl rv rr))
          ≤ 1 + max (depth (.node ll lv lr)) (depth (.node rl rv rr))
        omega

end LC.P0111
