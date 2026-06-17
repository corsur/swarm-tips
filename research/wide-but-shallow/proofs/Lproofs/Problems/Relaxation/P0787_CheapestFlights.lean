import Lproofs.Generators

/-! @lc 787 | name:Cheapest Flights Within K Stops | scheme:relaxation | family:bellman-ford |
    complexity:O(K·E) | source:https://leetcode.com/problems/cheapest-flights-within-k-stops/

    Bellman-Ford-style relaxation: repeatedly relax flight edges to lower each city's best cost.
    CLASSIFICATION: the set of cities reachable from the source under the flight relation `r` is the
    least fixpoint of one-step relaxation — the iterative edge-relaxation the bounded Bellman-Ford runs.
    `cls` certifies the relaxation fixpoint; `corr` ties it to the LEAST such set (no city relaxed in
    without a witnessing flight chain). DROP the cheapest-cost value and the K-stop bound. -/

namespace LC.P0787
open Interview

/-- Cities reachable from the source under the flight relation, as a relaxation lfp. -/
def sol {V : Type*} (src : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {src} r)

/-- Spec: the least relaxation-stable set — the minimal reachable set, no over-relaxation. -/
def spec {V : Type*} (src : V) (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (reachOp {src} r)) S

/-- SCHEME (relaxation): the reachable set is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (src : V) (r : V → V → Prop) : reachOp {src} r (sol src r) = sol src r :=
  reach_is_dp_fixpoint {src} r

/-- CORRECT: it is the LEAST fixpoint — the minimal Bellman-relaxed reachable set from the source. -/
theorem corr {V : Type*} (src : V) (r : V → V → Prop) : spec src r (sol src r) :=
  bellman_isLeast (reachOp {src} r)

end LC.P0787
