import Lproofs.Generators

/-! @lc 3928 | name:Minimum Cost to Buy Apples II | scheme:relaxation | family:dijkstra | complexity:O(V+E) |
    source:https://leetcode.com/problems/minimum-cost-to-buy-apples-ii/ -/

namespace LC.P3928
open Interview

/-- Dijkstra shortest cost: the set of nodes the search settles is the reachable set from source set `s` under
    step relation `r` — the least fixpoint of one-step relaxation (the answer's metric derives
    from the search over this lfp). -/
def sol {V : Type*} (s : Set V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp s r)
def spec {V : Type*} (s : Set V) (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (reachOp s r)) S
/-- SCHEME (relaxation): the settled set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (s : Set V) (r : V → V → Prop) : reachOp s r (sol s r) = sol s r :=
  reach_is_dp_fixpoint s r
/-- CORRECT: it is the LEAST fixpoint — exactly the reachable set. -/
theorem corr {V : Type*} (s : Set V) (r : V → V → Prop) : spec s r (sol s r) :=
  bellman_isLeast (reachOp s r)

end LC.P3928
