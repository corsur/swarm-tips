import Lproofs.Generators

/-! @lc 1584 | name:Min Cost to Connect All Points | scheme:relaxation | family:union-find |
    complexity:O(n² log n) | source:https://leetcode.com/problems/min-cost-to-connect-all-points/

    A minimum spanning tree connects all points; its connectivity substrate is which points are joined
    to a start point as edges are added. CLASSIFICATION: the points connected to `start` under the
    concrete chosen-edge relation `edge g` (`v` is joined to `u` by a selected edge) are the least
    fixpoint of one-step relaxation. `cls` certifies the relaxation fixpoint; `corr` proves the fixpoint
    is exactly the points connected to `start` along the edges — over a concrete relation, not a free
    `r`. (The minimum total cost is the algorithm's extra weighted output; reachability of all points is
    the spanning condition.) -/

namespace LC.P1584
open Interview

/-- Concrete spanning edge: `v` is joined to `u` by a selected edge (adjacency list `g`). -/
def edge (g : ℕ → List ℕ) (u v : ℕ) : Prop := v ∈ g u

/-- Points connected to `start` under the edges, as the least relaxation fixpoint. -/
def sol (g : ℕ → List ℕ) (start : ℕ) : Set ℕ := OrderHom.lfp (reachOp {start} (edge g))

/-- Spec: exactly the points connected to `start` along the selected edges. -/
def spec (g : ℕ → List ℕ) (start : ℕ) (S : Set ℕ) : Prop :=
  S = {v | Relation.ReflTransGen (edge g) start v}

/-- SCHEME (relaxation): the connected set is a fixpoint of one-step relaxation of the edges. -/
theorem cls (g : ℕ → List ℕ) (start : ℕ) :
    reachOp {start} (edge g) (sol g start) = sol g start :=
  reach_is_dp_fixpoint {start} (edge g)

/-- CORRECT: the relaxation lfp is exactly the points connected to `start` along the selected edges. -/
theorem corr (g : ℕ → List ℕ) (start : ℕ) : spec g start (sol g start) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P1584
