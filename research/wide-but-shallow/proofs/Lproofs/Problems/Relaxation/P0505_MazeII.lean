import Lproofs.Generators

/-! @lc 505 | name:The Maze II | scheme:relaxation | family:dijkstra | complexity:O(mn log mn) |
    source:https://leetcode.com/problems/the-maze-ii/

    A ball rolls until it hits a wall, producing a graph on stop cells. CLASSIFICATION: the stop cells
    reachable from `start` under the concrete roll relation `roll g` (`v` is a stop cell the ball
    reaches by rolling from `u`) are the least fixpoint of one-step relaxation. `cls` certifies the
    relaxation fixpoint; `corr` proves the fixpoint is exactly the stop cells reachable from `start` by
    rolling — over a concrete relation, not a free `r`. (The shortest roll-distance is the algorithm's
    extra weighted output.) -/

namespace LC.P0505
open Interview

/-- Concrete roll edge: `v` is a stop cell reached by rolling from `u` (adjacency list `g`). -/
def roll (g : ℕ → List ℕ) (u v : ℕ) : Prop := v ∈ g u

/-- Stop cells reachable from `start` by rolling, as the least relaxation fixpoint. -/
def sol (g : ℕ → List ℕ) (start : ℕ) : Set ℕ := OrderHom.lfp (reachOp {start} (roll g))

/-- Spec: exactly the stop cells reachable from `start` along roll edges. -/
def spec (g : ℕ → List ℕ) (start : ℕ) (S : Set ℕ) : Prop :=
  S = {v | Relation.ReflTransGen (roll g) start v}

/-- SCHEME (relaxation): the reachable stop set is a fixpoint of one-step relaxation of the roll graph. -/
theorem cls (g : ℕ → List ℕ) (start : ℕ) :
    reachOp {start} (roll g) (sol g start) = sol g start :=
  reach_is_dp_fixpoint {start} (roll g)

/-- CORRECT: the relaxation lfp is exactly the stop cells reachable from `start` by rolling. -/
theorem corr (g : ℕ → List ℕ) (start : ℕ) : spec g start (sol g start) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]


/-- A concrete roll graph (stop cells 0..3 of a small corridor maze): from stop 0 the ball can
    roll to stops 1 or 2; from 1 it reaches 3; stop 4 is walled off. -/
def exG : ℕ → List ℕ
  | 0 => [1, 2]
  | 1 => [3]
  | _ => []

/-- TEST VECTOR: destination stop 3 is reachable from the start by rolling, stop 4 is not. -/
theorem vec : 3 ∈ sol exG 0 ∧ 4 ∉ sol exG 0 := by
  have h := corr exG 0
  unfold spec at h
  rw [h]
  simp only [Set.mem_setOf_eq]
  constructor
  · exact .head (show roll exG 0 1 by simp [roll, exG])
      (.head (show roll exG 1 3 by simp [roll, exG]) .refl)
  · intro hr
    have bound : ∀ v, Relation.ReflTransGen (roll exG) 0 v → v ≤ 3 := by
      intro v hv
      induction hv with
      | refl => omega
      | @tail b c _ step ih =>
        interval_cases b <;> simp [roll, exG] at step <;> omega
    exact absurd (bound 4 hr) (by omega)

end LC.P0505
