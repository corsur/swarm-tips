import Lproofs.Generators

/-! @lc 863 | name:All Nodes Distance K in Binary Tree | scheme:relaxation | family:bfs |
    complexity:O(V+E) | source:https://leetcode.com/problems/all-nodes-distance-k-in-binary-tree/

    Re-rooting the tree as an undirected graph (adjacency `adj`, symmetric), the nodes at distance `K`
    from the target are read off the BFS hop-distance from the target `src`. CLASSIFICATION: the
    distance is the least step count under the concrete edge relation `edge adj` (`v` is a neighbour of
    `u`), and the BFS layers are exactly the bounded relaxation iterates. `cls` ties the scheme to the
    concrete layers; `corr` proves `sol` is the genuine least hop-distance in this graph — over a
    concrete relation, not a free `r`. -/

namespace LC.P0863
open Interview

/-- Concrete undirected edge: `v` is a neighbour of `u` in the adjacency `adj`. -/
def edge (adj : ℕ → List ℕ) (u v : ℕ) : Prop := v ∈ adj u

/-- BFS hop-distance: the least number of edge-steps from `src` reaching `v`. -/
noncomputable def sol (adj : ℕ → List ℕ) (src v : ℕ) : ℕ :=
  sInf {n | Reaches (edge adj) src v n}

/-- Spec: the answer is the least number of edge-steps reaching `v`. -/
def spec (adj : ℕ → List ℕ) (src v : ℕ) (d : ℕ) : Prop :=
  IsLeast {n | Reaches (edge adj) src v n} d

/-- SCHEME (relaxation): the BFS layers are exactly the bounded relaxation iterates of the concrete
    edge relation — reachable in `≤ n` steps is `(reachOp {src} (edge adj))^[n+1] ∅`. -/
theorem cls (adj : ℕ → List ℕ) (src : ℕ) :
    ∀ (n : ℕ) (v : ℕ), v ∈ (reachOp ({src} : Set ℕ) (edge adj))^[n + 1] (∅ : Set ℕ) ↔
      ∃ m ≤ n, Reaches (edge adj) src v m :=
  mem_reachOp_iterate src (edge adj)

/-- CORRECT: `sol` is the least number of edge-steps reaching `v` in this graph — the genuine BFS
    hop-distance from the target. -/
theorem corr (adj : ℕ → List ℕ) (src v : ℕ) (h : ∃ n, Reaches (edge adj) src v n) :
    spec adj src v (sol adj src v) :=
  ⟨Nat.sInf_mem h, fun _ hn => Nat.sInf_le hn⟩

end LC.P0863
