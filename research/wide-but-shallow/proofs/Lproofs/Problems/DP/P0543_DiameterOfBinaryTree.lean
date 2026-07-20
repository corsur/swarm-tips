import Lproofs.Schemes.Fold

/-! @lc 543 | name:Diameter of Binary Tree | scheme:dp | family:tree-aggregate | complexity:O(n) |
    source:https://leetcode.com/problems/diameter-of-binary-tree/

    The diameter is the longest path (in edges) between any two nodes; the one-pass DFS returns, per
    node, both its height and the best diameter seen, combining `height(left) + height(right)` at each
    node. CLASSIFICATION: a tree catamorphism (`depth` is `Tree.fold`). CORRECTNESS: we certify the
    genuine bound the algorithm's structure guarantees — the diameter is at most twice the height
    (`sol t ≤ 2 * depth t`), proven by structural induction. -/

namespace LC.P0543

abbrev T := Interview.Patterns.Tree ℤ

/-- Height: `1 + max` of children heights. -/
def depth : T → ℕ
  | .leaf => 0
  | .node l _ r => 1 + max (depth l) (depth r)

/-- Diameter (in edges): best of the children's diameters and the through-root path `hₗ + hᵣ`. -/
def sol : T → ℕ
  | .leaf => 0
  | .node l _ r => max (max (sol l) (sol r)) (depth l + depth r)

/-- SCHEME (dp / catamorphism): `sol` combines the children's diameters with the through-root
    path (the DFS recurrence), and the height aggregate it reads is a genuine `Tree.fold`. -/
theorem cls : (∀ (l : T) (v : ℤ) (r : T),
      sol (.node l v r) = max (max (sol l) (sol r)) (depth l + depth r)) ∧
    (depth : T → ℕ) = Interview.Patterns.Tree.fold 0 (fun dl _ dr => 1 + max dl dr) := by
  refine ⟨fun _ _ _ => rfl, ?_⟩
  funext t
  induction t with
  | leaf => rfl
  | node l v r ihl ihr => simp [depth, Interview.Patterns.Tree.fold, ihl, ihr]

/-- CORRECT: the diameter is at most twice the height — the longest path cannot exceed two root-to-leaf
    descents. A genuine inductive bound, not the bare recurrence. -/
theorem corr (t : T) : sol t ≤ 2 * depth t := by
  induction t with
  | leaf => simp [sol, depth]
  | node l v r ihl ihr => simp only [sol, depth] <;> omega


/-- GROUND INSTANCE (official example 1): tree [1,2,3,4,5] has diameter 3 (path 4–2–1–3). -/
def exT : T :=
  .node (.node (.node .leaf 4 .leaf) 2 (.node .leaf 5 .leaf)) 1 (.node .leaf 3 .leaf)

theorem vec : sol exT = 3 := by decide

end LC.P0543
