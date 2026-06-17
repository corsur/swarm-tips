import Lproofs.Generators

/-! @lc 2258 | name:Escape the Spreading Fire | scheme:relaxation | family:bfs | complexity:O(V+E) |
    source:https://leetcode.com/problems/escape-the-spreading-fire/ -/

namespace LC.P2258
open Interview

/-- BFS fire/person spread: the set of nodes the search visits is the reachable set from the source set `s`
    under the step relation `r` — the least fixpoint of one-step relaxation. (The problem's
    distance/time answer is the search depth over this lfp.) -/
def sol {V : Type*} (s : Set V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp s r)

/-- Spec: the least relaxation-stable visited set (everything reachable from the sources). -/
def spec {V : Type*} (s : Set V) (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (reachOp s r)) S

/-- SCHEME (relaxation): the visited set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (s : Set V) (r : V → V → Prop) : reachOp s r (sol s r) = sol s r :=
  reach_is_dp_fixpoint s r

/-- CORRECT: it is the LEAST fixpoint — exactly the reachable set, none visited without a path. -/
theorem corr {V : Type*} (s : Set V) (r : V → V → Prop) : spec s r (sol s r) :=
  bellman_isLeast (reachOp s r)

end LC.P2258
