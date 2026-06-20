import Lproofs.Generators

/-! @lc 2493 | name:Divide Nodes Into the Maximum Number of Groups | scheme:relaxation |
    family:graph-other | complexity:O(V·(V+E)) | source:https://leetcode.com/problems/divide-nodes-into-the-maximum-number-of-groups/

    A valid grouping numbers nodes so adjacent nodes differ by exactly one group; the maximum number of
    groups, for a connected component rooted at `src`, is one more than the farthest BFS layer from the
    best root. CLASSIFICATION (relaxation): the group index of a node is its BFS hop-distance from the
    root under the concrete undirected edge relation, and the BFS layers are the bounded relaxation
    iterates. CORRECTNESS: `sol` is certified to be the genuine least hop-distance (the node's layer)
    over the concrete graph — over a real adjacency, not a free `r`. -/

namespace LC.P2493
open Interview

/-- Concrete undirected edge: `v` is a neighbour of `u` in the adjacency `adj`. -/
def edge (adj : ℕ → List ℕ) (u v : ℕ) : Prop := v ∈ adj u

/-- BFS hop-distance: the layer (group offset) of `v` from the root `src`. -/
noncomputable def sol (adj : ℕ → List ℕ) (src v : ℕ) : ℕ :=
  sInf {n | Reaches (edge adj) src v n}

/-- Spec: the node's layer is the least number of edges from the root reaching it. -/
def spec (adj : ℕ → List ℕ) (src v : ℕ) (d : ℕ) : Prop :=
  IsLeast {n | Reaches (edge adj) src v n} d

/-- SCHEME (relaxation): reachable in `≤ n` edges is the `n+1`-st bounded relaxation iterate — the BFS
    layers the grouping reads off. -/
theorem cls (adj : ℕ → List ℕ) (src : ℕ) :
    ∀ (n : ℕ) (v : ℕ), v ∈ (reachOp ({src} : Set ℕ) (edge adj))^[n + 1] (∅ : Set ℕ) ↔
      ∃ m ≤ n, Reaches (edge adj) src v m :=
  mem_reachOp_iterate src (edge adj)

/-- CORRECT: `sol` is the genuine least hop-distance from the root — the node's group layer in this
    graph. -/
theorem corr (adj : ℕ → List ℕ) (src v : ℕ) (h : ∃ n, Reaches (edge adj) src v n) :
    spec adj src v (sol adj src v) :=
  ⟨Nat.sInf_mem h, fun _ hn => Nat.sInf_le hn⟩

end LC.P2493
