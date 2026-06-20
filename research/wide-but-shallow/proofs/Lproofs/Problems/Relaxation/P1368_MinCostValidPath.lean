import Lproofs.Generators

/-! @lc 1368 | name:Minimum Cost to Make at Least One Valid Path in a Grid | scheme:relaxation |
    family:dijkstra | complexity:O(mn) | source:https://leetcode.com/problems/minimum-cost-to-make-at-least-one-valid-path-in-a-grid/

    0-1 BFS / Dijkstra relaxes the cost to redirect arrows along a path; the connectivity substrate is
    which cells are reachable once arrows may be redirected. CLASSIFICATION: the cells reachable from
    `start` under the concrete move relation `move g` (`v` is a reachable neighbour cell) are the least
    fixpoint of one-step relaxation. `cls` certifies the relaxation fixpoint; `corr` proves the fixpoint
    is exactly the cells reachable from `start` along moves — over a concrete relation, not a free `r`.
    (The minimum redirection cost is the algorithm's extra weighted output.) -/

namespace LC.P1368
open Interview

/-- Concrete move edge: `v` is a reachable neighbour cell of `u` (adjacency list `g`). -/
def move (g : ℕ → List ℕ) (u v : ℕ) : Prop := v ∈ g u

/-- Cells reachable from `start` under moves, as the least relaxation fixpoint. -/
def sol (g : ℕ → List ℕ) (start : ℕ) : Set ℕ := OrderHom.lfp (reachOp {start} (move g))

/-- Spec: exactly the cells reachable from `start` along move edges. -/
def spec (g : ℕ → List ℕ) (start : ℕ) (S : Set ℕ) : Prop :=
  S = {v | Relation.ReflTransGen (move g) start v}

/-- SCHEME (relaxation): the reachable cell set is a fixpoint of one-step relaxation. -/
theorem cls (g : ℕ → List ℕ) (start : ℕ) :
    reachOp {start} (move g) (sol g start) = sol g start :=
  reach_is_dp_fixpoint {start} (move g)

/-- CORRECT: the relaxation lfp is exactly the cells reachable from `start` along moves. -/
theorem corr (g : ℕ → List ℕ) (start : ℕ) : spec g start (sol g start) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P1368
