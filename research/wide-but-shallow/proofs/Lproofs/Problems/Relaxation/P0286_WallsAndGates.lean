import Lproofs.Generators

/-! @lc 286 | name:Walls and Gates | scheme:relaxation | family:bfs | complexity:O(mn) |
    source:https://leetcode.com/problems/walls-and-gates/

    Distances fill outward from the gates through open rooms (avoiding walls). CLASSIFICATION: the set
    of rooms a gate can fill is exactly the cells reachable from the gate sources under the open-room
    adjacency relation — the least fixpoint of one-step relaxation (the BFS frontier). `cls` certifies
    it is a relaxation fixpoint; `corr` ties the fixpoint to genuine reachability. We certify the
    fill structure, not the shortest distances. -/

namespace LC.P0286
open Interview

/-- Fill steps to an adjacent open (non-wall) room. -/
def fill (isOpen : ℕ → Prop) (adj : ℕ → ℕ → Prop) (p q : ℕ) : Prop := adj p q ∧ isOpen q

/-- The rooms reached: cells reachable from the gate `sources`, as the relaxation lfp. -/
def sol (isOpen : ℕ → Prop) (adj : ℕ → ℕ → Prop) (sources : Set ℕ) : Set ℕ :=
  OrderHom.lfp (reachOp sources (fill isOpen adj))

/-- Spec: exactly the cells reachable from a gate along open-room adjacency. -/
def spec (isOpen : ℕ → Prop) (adj : ℕ → ℕ → Prop) (sources : Set ℕ) (S : Set ℕ) : Prop :=
  S = {v | ∃ u ∈ sources, Relation.ReflTransGen (fill isOpen adj) u v}

/-- SCHEME (relaxation): the reached set is a fixpoint of one-step relaxation. -/
theorem cls (isOpen : ℕ → Prop) (adj : ℕ → ℕ → Prop) (sources : Set ℕ) :
    reachOp sources (fill isOpen adj) (sol isOpen adj sources) = sol isOpen adj sources :=
  reach_is_dp_fixpoint sources (fill isOpen adj)

/-- CORRECT: the relaxation lfp is exactly the set reachable from the gates. -/
theorem corr (isOpen : ℕ → Prop) (adj : ℕ → ℕ → Prop) (sources : Set ℕ) :
    spec isOpen adj sources (sol isOpen adj sources) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]

end LC.P0286
