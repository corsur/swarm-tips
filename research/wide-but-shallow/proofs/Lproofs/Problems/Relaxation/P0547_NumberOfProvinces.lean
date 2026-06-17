import Lproofs.Generators

/-! @lc 547 | name:Number of Provinces | scheme:relaxation | family:union-find | complexity:O(V^2) |
    source:https://leetcode.com/problems/number-of-provinces/

    A province is a connected component of the friendship graph; the answer counts them. CLASSIFICATION:
    the component containing a city `c` is exactly the set reachable from `{c}` under the (symmetric)
    friendship relation `r` — the least fixpoint of one-step relaxation (union-find / BFS). `cls`
    certifies the relaxation fixpoint; `corr` ties it to the LEAST such set (no city joined without a
    witnessing friendship chain). DROP the component count. -/

namespace LC.P0547
open Interview

/-- The province of `c`: the reachable set from `{c}` under friendship, as a relaxation lfp. -/
def sol {V : Type*} (c : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {c} r)

/-- Spec: the least relaxation-stable set — the minimal component, no over-relaxation. -/
def spec {V : Type*} (c : V) (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (reachOp {c} r)) S

/-- SCHEME (relaxation): the province is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (c : V) (r : V → V → Prop) : reachOp {c} r (sol c r) = sol c r :=
  reach_is_dp_fixpoint {c} r

/-- CORRECT: it is the LEAST fixpoint — the minimal connected component containing `c`. -/
theorem corr {V : Type*} (c : V) (r : V → V → Prop) : spec c r (sol c r) :=
  bellman_isLeast (reachOp {c} r)

end LC.P0547
