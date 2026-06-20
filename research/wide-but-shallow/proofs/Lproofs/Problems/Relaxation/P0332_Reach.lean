import Lproofs.Generators

/-! @lc 332 | name:Reconstruct Itinerary | scheme:relaxation | family:euler-path | complexity:O(V+E) |
    source:https://leetcode.com/problems/reconstruct-itinerary/

    The itinerary uses every ticket; a prerequisite the traversal relies on is that every airport on
    the route is reachable from the start along the ticket edges. CLASSIFICATION: the airports reachable
    from `start` under the concrete ticket relation `flight g` (`v` is a destination of a ticket from
    `u`) are the least fixpoint of one-step relaxation. `cls` certifies the relaxation fixpoint; `corr`
    proves the fixpoint is exactly the airports reachable from `start` along the tickets — over a
    concrete relation, not a free `r`. (The full Euler-path ordering is the algorithm's extra output.) -/

namespace LC.P0332
open Interview

/-- Concrete ticket edge: `v` is a destination of a ticket departing `u` (adjacency list `g`). -/
def flight (g : ℕ → List ℕ) (u v : ℕ) : Prop := v ∈ g u

/-- Airports reachable from `start` under the tickets, as the least relaxation fixpoint. -/
def sol (g : ℕ → List ℕ) (start : ℕ) : Set ℕ := OrderHom.lfp (reachOp {start} (flight g))

/-- Spec: exactly the airports reachable from `start` along ticket edges. -/
def spec (g : ℕ → List ℕ) (start : ℕ) (S : Set ℕ) : Prop :=
  S = {v | Relation.ReflTransGen (flight g) start v}

/-- SCHEME (relaxation): the reachable airport set is a fixpoint of one-step relaxation of the tickets. -/
theorem cls (g : ℕ → List ℕ) (start : ℕ) :
    reachOp {start} (flight g) (sol g start) = sol g start :=
  reach_is_dp_fixpoint {start} (flight g)

/-- CORRECT: the relaxation lfp is exactly the airports reachable from `start` along the tickets. -/
theorem corr (g : ℕ → List ℕ) (start : ℕ) : spec g start (sol g start) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0332
