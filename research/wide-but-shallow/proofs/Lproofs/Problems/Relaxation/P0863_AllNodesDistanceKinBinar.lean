import Lproofs.Generators

/-! @lc 863 | name:All Nodes Distance K in Binary Tree | scheme:relaxation | family:bfs |
    complexity:O(V+E) | source:https://leetcode.com/problems/all-nodes-distance-k-in-binary-tree/

    Re-rooting the tree as an undirected graph, the nodes at distance `K` from the target are read
    off the BFS hop-distance: the least number of edge-steps (`r`) from the source node `src`. We
    model that distance and tie it to the relaxation scheme via the BFS-layer characterization
    (reachable in `≤ n` steps `=` `(reachOp {src} r)^[n+1] ∅`). -/

namespace LC.P0863
open Interview

/-- BFS hop-distance: the least number of edge-steps from `src` reaching `v`. -/
noncomputable def sol {V : Type*} (src : V) (r : V → V → Prop) (v : V) : ℕ :=
  sInf {n | Reaches r src v n}

/-- Spec: the answer is the least number of edge-steps reaching `v`. -/
def spec {V : Type*} (src : V) (r : V → V → Prop) (v : V) (d : ℕ) : Prop :=
  IsLeast {n | Reaches r src v n} d

/-- SCHEME (relaxation): the BFS layers are exactly the bounded relaxation iterates — reachable in
    `≤ n` steps is `(reachOp {src} r)^[n+1] ∅`. The hop-distance is read off these layers. -/
theorem cls {V : Type*} (src : V) (r : V → V → Prop) :
    ∀ (n : ℕ) (v : V), v ∈ (reachOp ({src} : Set V) r)^[n + 1] (∅ : Set V) ↔
      ∃ m ≤ n, Reaches r src v m :=
  mem_reachOp_iterate src r

/-- CORRECT: `sol` is the least number of edge-steps reaching `v` — the genuine BFS distance. -/
theorem corr {V : Type*} (src : V) (r : V → V → Prop) (v : V) (h : ∃ n, Reaches r src v n) :
    spec src r v (sol src r v) :=
  ⟨Nat.sInf_mem h, fun _ hn => Nat.sInf_le hn⟩

end LC.P0863
