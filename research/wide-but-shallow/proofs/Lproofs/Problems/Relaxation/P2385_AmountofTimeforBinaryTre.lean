import Lproofs.Generators

/-! @lc 2385 | name:Amount of Time for Binary Tree to Be Infected | scheme:relaxation | family:bfs |
    complexity:O(V+E) |
    source:https://leetcode.com/problems/amount-of-time-for-binary-tree-to-be-infected/

    Infection spreads one edge per minute from the start node; the time to infect the whole tree is
    the maximum BFS hop-distance. The core computation is that distance: the least number of
    edge-steps (`r`) from the source `src`, tied to the relaxation scheme via the BFS-layer
    characterization (reachable in `≤ n` steps `=` `(reachOp {src} r)^[n+1] ∅`). -/

namespace LC.P2385
open Interview

/-- BFS hop-distance: the least number of edge-steps from `src` reaching `v`. -/
noncomputable def sol {V : Type*} (src : V) (r : V → V → Prop) (v : V) : ℕ :=
  sInf {n | Reaches r src v n}

/-- Spec: the infection reaches `v` in the least number of edge-steps. -/
def spec {V : Type*} (src : V) (r : V → V → Prop) (v : V) (d : ℕ) : Prop :=
  IsLeast {n | Reaches r src v n} d

/-- SCHEME (relaxation): the BFS layers are exactly the bounded relaxation iterates — reachable in
    `≤ n` steps is `(reachOp {src} r)^[n+1] ∅`. The infection time is read off these layers. -/
theorem cls {V : Type*} (src : V) (r : V → V → Prop) :
    ∀ (n : ℕ) (v : V), v ∈ (reachOp ({src} : Set V) r)^[n + 1] (∅ : Set V) ↔
      ∃ m ≤ n, Reaches r src v m :=
  mem_reachOp_iterate src r

/-- CORRECT: `sol` is the least number of edge-steps the infection takes to reach `v`. -/
theorem corr {V : Type*} (src : V) (r : V → V → Prop) (v : V) (h : ∃ n, Reaches r src v n) :
    spec src r v (sol src r v) :=
  ⟨Nat.sInf_mem h, fun _ hn => Nat.sInf_le hn⟩

end LC.P2385
