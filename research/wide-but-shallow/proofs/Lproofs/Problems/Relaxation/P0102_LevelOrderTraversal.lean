import Lproofs.Generators

/-! @lc 102 | name:Binary Tree Level Order Traversal | scheme:relaxation | family:bfs | complexity:O(n) |
    source:https://leetcode.com/problems/binary-tree-level-order-traversal/ -/

namespace LC.P0102
open Interview

/-- `child u v`: `v` is a child of `u`. BFS visits exactly the nodes reachable from the root set
    `s` — the least fixpoint of one-step relaxation (the level grouping orders this set by depth). -/
def sol {V : Type*} (s : Set V) (child : V → V → Prop) : Set V := OrderHom.lfp (reachOp s child)

/-- Spec: the least relaxation-stable set of visited nodes (everything reachable from the root). -/
def spec {V : Type*} (s : Set V) (child : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (reachOp s child)) S

/-- SCHEME (relaxation): the visited set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (s : Set V) (child : V → V → Prop) :
    reachOp s child (sol s child) = sol s child :=
  reach_is_dp_fixpoint s child

/-- CORRECT: it is the LEAST fixpoint — exactly the reachable nodes, none visited without a path. -/
theorem corr {V : Type*} (s : Set V) (child : V → V → Prop) : spec s child (sol s child) :=
  bellman_isLeast (reachOp s child)

end LC.P0102
