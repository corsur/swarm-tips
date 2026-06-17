import Lproofs.Generators

/-! @lc 305 | name:Number of Islands II | scheme:relaxation | family:union-find | complexity:O(k·α) |
    source:https://leetcode.com/problems/number-of-islands-ii/

    After each land addition, report the island count; union-find merges newly-adjacent land cells.
    CLASSIFICATION: the island containing a land cell `c` is exactly the set reachable from `{c}` under
    the (symmetric) land-adjacency relation `r` — the least fixpoint of one-step relaxation (union-find).
    `cls` certifies the relaxation fixpoint; `corr` ties it to the LEAST such set (no cell merged without
    a witnessing adjacency chain). DROP the running island count. -/

namespace LC.P0305
open Interview

/-- The island of `c`: the reachable set from `{c}` under land-adjacency, as a relaxation lfp. -/
def sol {V : Type*} (c : V) (r : V → V → Prop) : Set V := OrderHom.lfp (reachOp {c} r)

/-- Spec: the least relaxation-stable set — the minimal island, no over-relaxation. -/
def spec {V : Type*} (c : V) (r : V → V → Prop) (S : Set V) : Prop :=
  IsLeast (Function.fixedPoints (reachOp {c} r)) S

/-- SCHEME (relaxation): the island is a fixpoint of one-step relaxation. -/
theorem cls {V : Type*} (c : V) (r : V → V → Prop) : reachOp {c} r (sol c r) = sol c r :=
  reach_is_dp_fixpoint {c} r

/-- CORRECT: it is the LEAST fixpoint — the minimal connected island containing `c`. -/
theorem corr {V : Type*} (c : V) (r : V → V → Prop) : spec c r (sol c r) :=
  bellman_isLeast (reachOp {c} r)

end LC.P0305
