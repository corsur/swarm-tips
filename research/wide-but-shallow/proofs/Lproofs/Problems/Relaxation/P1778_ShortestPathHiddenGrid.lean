import Lproofs.Generators

/-! @lc 1778 | name:Shortest Path in a Hidden Grid | scheme:relaxation | family:bfs | complexity:O(mn) |
    source:https://leetcode.com/problems/shortest-path-in-a-hidden-grid/

    Each move is one unit step to an open neighbour, so the shortest path length is the BFS hop-distance
    from the start. CLASSIFICATION: the distance is the least step count under the concrete open-move
    relation `move g` (`v` is an open neighbour of `u`), and the BFS layers are the bounded relaxation
    iterates. `cls` ties the scheme to the concrete layers; `corr` proves `sol` is the genuine least
    number of moves to reach the target — over a concrete relation, not a free `r`. -/

namespace LC.P1778
open Interview

/-- Concrete open-move edge: `v` is an open neighbour of `u` (adjacency list `g`). -/
def move (g : ℕ → List ℕ) (u v : ℕ) : Prop := v ∈ g u

/-- Shortest path length: the least number of open moves from `start` reaching `v`. -/
noncomputable def sol (g : ℕ → List ℕ) (start v : ℕ) : ℕ :=
  sInf {n | Reaches (move g) start v n}

/-- Spec: the answer is the least number of moves reaching `v`. -/
def spec (g : ℕ → List ℕ) (start v : ℕ) (d : ℕ) : Prop :=
  IsLeast {n | Reaches (move g) start v n} d

/-- SCHEME (relaxation): the BFS layers are exactly the bounded relaxation iterates of the concrete
    move relation — reachable in `≤ n` steps is `(reachOp {start} (move g))^[n+1] ∅`. -/
theorem cls (g : ℕ → List ℕ) (start : ℕ) :
    ∀ (n : ℕ) (v : ℕ), v ∈ (reachOp ({start} : Set ℕ) (move g))^[n + 1] (∅ : Set ℕ) ↔
      ∃ m ≤ n, Reaches (move g) start v m :=
  mem_reachOp_iterate start (move g)

/-- CORRECT: `sol` is the least number of open moves reaching `v` — the genuine shortest path length. -/
theorem corr (g : ℕ → List ℕ) (start v : ℕ) (h : ∃ n, Reaches (move g) start v n) :
    spec g start v (sol g start v) :=
  ⟨Nat.sInf_mem h, fun _ hn => Nat.sInf_le hn⟩

end LC.P1778
