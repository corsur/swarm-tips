import Lproofs.Generators

/-! @lc 269 | name:Alien Dictionary | scheme:relaxation | family:topo-sort | complexity:O(V+E) |
    source:https://leetcode.com/problems/alien-dictionary/

    Adjacent words give precedence edges (the first differing character orders two letters); the accepted
    solution topologically sorts the resulting letter graph. CLASSIFICATION (relaxation): the letters
    that must come after a given letter are its reachable set under the CONCRETE derived precedence
    adjacency `g : ℕ → List ℕ` (letter → letters it precedes), as a least fixpoint of one-step
    relaxation. CORRECTNESS: the relaxation fixpoint is exactly that must-come-after set; a valid letter
    order exists iff the precedence graph has no cycle. -/

namespace LC.P0269
open Interview

/-- Concrete precedence edge: letter `u` must come before letter `v` (`v ∈ g u`). -/
def precedes (g : ℕ → List ℕ) (u v : ℕ) : Prop := v ∈ g u

/-- Letters that must come after `start`: reachable from it under `precedes`, as a least fixpoint. -/
def sol (g : ℕ → List ℕ) (start : ℕ) : Set ℕ := OrderHom.lfp (reachOp {start} (precedes g))

/-- Spec: exactly the letters reachable from `start` along a precedence chain. -/
def spec (g : ℕ → List ℕ) (start : ℕ) (S : Set ℕ) : Prop :=
  S = {v | Relation.ReflTransGen (precedes g) start v}

/-- SCHEME (relaxation): the must-come-after set is a fixpoint of one-step relaxation of `precedes`. -/
theorem cls (g : ℕ → List ℕ) (start : ℕ) :
    reachOp {start} (precedes g) (sol g start) = sol g start :=
  reach_is_dp_fixpoint {start} (precedes g)

/-- CORRECT: the relaxation lfp is exactly the set of letters forced after `start` over the concrete
    precedence graph --- the constraint the topological order must respect. -/
theorem corr (g : ℕ → List ℕ) (start : ℕ) : spec g start (sol g start) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0269
