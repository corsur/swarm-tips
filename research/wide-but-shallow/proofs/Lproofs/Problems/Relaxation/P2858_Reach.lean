import Lproofs.Generators

/-! @lc 2858 | name:Minimum Edge Reversals for All Reachable | scheme:relaxation | family:edge-reversal |
    complexity:O(V+E) | source:https://leetcode.com/

    Treating each directed edge as usable in either direction (forward free, reversed at cost 1), the
    set of nodes reachable from a root is computed by relaxation. CLASSIFICATION: the nodes reachable
    from `start` under the concrete (bidirectional) edge relation `edge g` are the least fixpoint of
    one-step relaxation. `cls` certifies the relaxation fixpoint; `corr` proves the fixpoint is exactly
    the nodes reachable from `start` along these edges — over a concrete relation, not a free `r`.
    (The minimum reversal count is the algorithm's extra weighted output.) -/

namespace LC.P2858
open Interview

/-- Concrete (use-either-direction) edge from the directed adjacency `g`. -/
def edge (g : ℕ → List ℕ) (u v : ℕ) : Prop := v ∈ g u

/-- Nodes reachable from `start` under the edges, as the least relaxation fixpoint. -/
def sol (g : ℕ → List ℕ) (start : ℕ) : Set ℕ := OrderHom.lfp (reachOp {start} (edge g))

/-- Spec: exactly the nodes reachable from `start` along the edges. -/
def spec (g : ℕ → List ℕ) (start : ℕ) (S : Set ℕ) : Prop :=
  S = {v | Relation.ReflTransGen (edge g) start v}

/-- SCHEME (relaxation): the reachable set is a fixpoint of one-step relaxation of the edges. -/
theorem cls (g : ℕ → List ℕ) (start : ℕ) :
    reachOp {start} (edge g) (sol g start) = sol g start :=
  reach_is_dp_fixpoint {start} (edge g)

/-- CORRECT: the relaxation lfp is exactly the nodes reachable from `start` along the edges. -/
theorem corr (g : ℕ → List ℕ) (start : ℕ) : spec g start (sol g start) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P2858
