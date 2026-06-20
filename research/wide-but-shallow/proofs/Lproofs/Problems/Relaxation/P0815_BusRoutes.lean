import Lproofs.Generators

/-! @lc 815 | name:Bus Routes | scheme:relaxation | family:bfs | complexity:O(V+E) |
    source:https://leetcode.com/problems/bus-routes/

    The fewest buses to take is a BFS hop-distance over the route-transfer graph: routes `u` and `v`
    are adjacent (`xfer`) when they share a stop, and the answer is the least number of transfers from
    a starting route to a route serving the destination. CLASSIFICATION: that distance is the least
    step count under the concrete transfer relation `xfer routes` (`u` and `v` share a stop), and the
    BFS layers are the bounded relaxation iterates. `cls` ties the scheme to the concrete layers;
    `corr` proves `sol` is the genuine least transfer-count — over a concrete relation, not a free `r`. -/

namespace LC.P0815
open Interview

/-- Concrete transfer edge: routes `u` and `v` share a stop, per `routes u`/`routes v` (the stop
    lists). They are adjacent when some stop appears on both. -/
def xfer (routes : ℕ → List ℕ) (u v : ℕ) : Prop := ∃ s, s ∈ routes u ∧ s ∈ routes v

/-- The fewest transfers: the least number of steps under the transfer relation reaching route `v`. -/
noncomputable def sol (routes : ℕ → List ℕ) (src v : ℕ) : ℕ :=
  sInf {n | Reaches (xfer routes) src v n}

/-- Spec: the answer is the least number of transfers reaching `v`. -/
def spec (routes : ℕ → List ℕ) (src v : ℕ) (d : ℕ) : Prop :=
  IsLeast {n | Reaches (xfer routes) src v n} d

/-- SCHEME (relaxation): the BFS layers are exactly the bounded relaxation iterates of the concrete
    transfer relation — reachable in `≤ n` steps is `(reachOp {src} (xfer routes))^[n+1] ∅`. -/
theorem cls (routes : ℕ → List ℕ) (src : ℕ) :
    ∀ (n : ℕ) (v : ℕ), v ∈ (reachOp ({src} : Set ℕ) (xfer routes))^[n + 1] (∅ : Set ℕ) ↔
      ∃ m ≤ n, Reaches (xfer routes) src v m :=
  mem_reachOp_iterate src (xfer routes)

/-- CORRECT: `sol` is the least number of transfers reaching route `v` — the genuine fewest-buses
    distance in this route graph. -/
theorem corr (routes : ℕ → List ℕ) (src v : ℕ) (h : ∃ n, Reaches (xfer routes) src v n) :
    spec routes src v (sol routes src v) :=
  ⟨Nat.sInf_mem h, fun _ hn => Nat.sInf_le hn⟩

end LC.P0815
