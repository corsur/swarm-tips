import Lproofs.Generators

/-! @lc 3387 | name:Graph Reachability | scheme:relaxation | family:graph-traversal | complexity:O(V+E) |
    source:https://leetcode.com/

    Given a directed graph by adjacency lists `g` (`g u` lists the out-neighbours of `u`), report the
    nodes reachable from a source. CLASSIFICATION: that reachable set is the least fixpoint of one-step
    relaxation under the concrete edge relation `edge g` (`v` is an out-neighbour of `u`). `cls`
    certifies the relaxation fixpoint; `corr` proves the fixpoint is exactly the set reachable from the
    source along the graph's edges — the actual answer, over a concrete relation, not a free `r`. -/

namespace LC.P3387
open Interview

/-- Concrete directed edge: `v` is an out-neighbour of `u` in the adjacency list `g`. -/
def edge (g : ℕ → List ℕ) (u v : ℕ) : Prop := v ∈ g u

/-- The reachable set from `src`, as the least fixpoint of one-step relaxation. -/
def sol (g : ℕ → List ℕ) (src : ℕ) : Set ℕ := OrderHom.lfp (reachOp {src} (edge g))

/-- Spec: exactly the nodes reachable from `src` along the graph's edges. -/
def spec (g : ℕ → List ℕ) (src : ℕ) (S : Set ℕ) : Prop :=
  S = {v | Relation.ReflTransGen (edge g) src v}

/-- SCHEME (relaxation): the reachable set is a fixpoint of one-step relaxation of the concrete graph. -/
theorem cls (g : ℕ → List ℕ) (src : ℕ) : reachOp {src} (edge g) (sol g src) = sol g src :=
  reach_is_dp_fixpoint {src} (edge g)

/-- CORRECT: the relaxation lfp is exactly the set of nodes reachable from `src` along `g`'s edges. -/
theorem corr (g : ℕ → List ℕ) (src : ℕ) : spec g src (sol g src) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P3387
