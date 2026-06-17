import Lproofs.Generators

/-! @lc 959 | name:Regions Cut By Slashes | scheme:relaxation | family:union-find | complexity:O(n^2) |
    source:https://leetcode.com/problems/regions-cut-by-slashes/ -/

namespace LC.P0959
open Interview

/-- `r u v`: sub-cells `u`, `v` are directly connected (no slash between them). A region is a
    connected component — the reachable set from its cells `s` under `r`, the least fixpoint of
    one-step relaxation (union-find). -/
def sol {V : Type*} (s : Set V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp s r)

/-- Spec: the least relaxation-stable connected region. -/
def spec {V : Type*} (s : Set V) (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (reachOp s r)) S

/-- SCHEME (relaxation): the region is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (s : Set V) (r : V → V → Prop) : reachOp s r (sol s r) = sol s r :=
  reach_is_dp_fixpoint s r

/-- CORRECT: it is the LEAST fixpoint — the minimal region, no cell reached without a connection chain. -/
theorem corr {V : Type*} (s : Set V) (r : V → V → Prop) : spec s r (sol s r) :=
  bellman_isLeast (reachOp s r)

end LC.P0959
