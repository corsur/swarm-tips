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

/-- SCHEME (relaxation): the BFS layers decide the group offset — node `v` lies in the `n+1`-st
    bounded relaxation iterate iff `sol` (its hop-distance layer) is at most `n`. -/
theorem cls (adj : ℕ → List ℕ) (src v : ℕ) (h : ∃ n, Reaches (edge adj) src v n) (n : ℕ) :
    v ∈ (reachOp ({src} : Set ℕ) (edge adj))^[n + 1] (∅ : Set ℕ) ↔ sol adj src v ≤ n := by
  rw [mem_reachOp_iterate src (edge adj) n v]
  constructor
  · rintro ⟨m, hmn, hm⟩
    exact le_trans (Nat.sInf_le hm) hmn
  · intro hle
    exact ⟨sol adj src v, hle, Nat.sInf_mem h⟩

/-- CORRECT: `sol` is the genuine least hop-distance from the root — the node's group layer in this
    graph. -/
theorem corr (adj : ℕ → List ℕ) (src v : ℕ) (h : ∃ n, Reaches (edge adj) src v n) :
    spec adj src v (sol adj src v) :=
  ⟨Nat.sInf_mem h, fun _ hn => Nat.sInf_le hn⟩


/-- Official example 1 adjacency (1-indexed): edges [1,2],[1,4],[1,5],[2,6],[2,3],[4,6]. -/
def exAdj : ℕ → List ℕ
  | 1 => [2, 4, 5]
  | 2 => [1, 6, 3]
  | 3 => [2]
  | 4 => [1, 6]
  | 5 => [1]
  | 6 => [2, 4]
  | _ => []

/-- GROUND INSTANCE (official example 1): node 6 sits two hops from node 1 (1→2→6), and not
    fewer — its BFS layer, which the grouping reads off. -/
theorem vec : sol exAdj 1 6 = 2 := by
  have hleast : IsLeast {n | Reaches (edge exAdj) 1 6 n} 2 := by
    constructor
    · show Reaches (edge exAdj) 1 6 2
      exact .step (.step .refl (show (2 : ℕ) ∈ exAdj 1 by decide))
        (show (6 : ℕ) ∈ exAdj 2 by decide)
    · intro m hm
      by_contra hlt
      push_neg at hlt
      interval_cases m
      · cases hm
      · cases hm with
        | step h0 he =>
          cases h0
          simp [edge, exAdj] at he
  exact hleast.csInf_eq

end LC.P2493
