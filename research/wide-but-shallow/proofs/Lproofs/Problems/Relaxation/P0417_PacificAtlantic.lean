import Lproofs.Generators

/-! @lc 417 | name:Pacific Atlantic Water Flow | scheme:relaxation | family:dfs-flood |
    complexity:O(V) | source:https://leetcode.com/problems/pacific-atlantic-water-flow/ -/

namespace LC.P0417
open Interview

/-- `flow u v`: water can move from `u` to `v` (a not-higher neighbour). The cells that drain to a
    given ocean are those reachable from its border cells `s` under `flow` — a BFS/DFS flood that
    is the least fixpoint of one-step relaxation. (The final answer intersects the two oceans'
    reachable sets; each is this lfp.) -/
def sol {V : Type*} (s : Set V) (flow : V → V → Prop) : Set V := OrderHom.lfp (reachOp s flow)

/-- Spec: the least relaxation-stable set draining to the ocean (the reachable flood). -/
def spec {V : Type*} (s : Set V) (flow : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (reachOp s flow)) S

/-- SCHEME (relaxation): the drainage set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (s : Set V) (flow : V → V → Prop) : reachOp s flow (sol s flow) = sol s flow :=
  reach_is_dp_fixpoint s flow

/-- CORRECT: it is the LEAST fixpoint — the minimal flood, no cell included without a flow path. -/
theorem corr {V : Type*} (s : Set V) (flow : V → V → Prop) : spec s flow (sol s flow) :=
  bellman_isLeast (reachOp s flow)

end LC.P0417
