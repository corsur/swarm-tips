import Lproofs.Generators

/-! @lc 1719 | name:Number Of Ways To Reconstruct A Tree | scheme:relaxation | family:graph-other |
    complexity:O(V+E) | source:https://leetcode.com/problems/number-of-ways-to-reconstruct-a-tree/

    Reconstruction reasons over ancestor/descendant reachability in the pair graph. CLASSIFICATION: the
    descendants of a node are the nodes reachable from it under the concrete ancestor adjacency `desc g`
    (`v` is a recorded child of `u`) — the least fixpoint of one-step relaxation. `cls` certifies the
    relaxation fixpoint; `corr` proves the fixpoint is exactly the nodes reachable along the recorded
    ancestry — over a concrete relation, not a free `r`. (The counting/uniqueness verdict is extra.) -/

namespace LC.P1719
open Interview

/-- Concrete ancestry edge: `v` is a recorded child of `u` (adjacency list `g`). -/
def desc (g : ℕ → List ℕ) (u v : ℕ) : Prop := v ∈ g u

/-- Nodes reachable (descendants) from `start` under the ancestry, as the least relaxation fixpoint. -/
def sol (g : ℕ → List ℕ) (start : ℕ) : Set ℕ := OrderHom.lfp (reachOp {start} (desc g))

/-- Spec: exactly the nodes reachable from `start` along the recorded ancestry. -/
def spec (g : ℕ → List ℕ) (start : ℕ) (S : Set ℕ) : Prop :=
  S = {v | Relation.ReflTransGen (desc g) start v}

/-- SCHEME (relaxation): the descendant set is a fixpoint of one-step relaxation of the ancestry. -/
theorem cls (g : ℕ → List ℕ) (start : ℕ) :
    reachOp {start} (desc g) (sol g start) = sol g start :=
  reach_is_dp_fixpoint {start} (desc g)

/-- CORRECT: the relaxation lfp is exactly the nodes reachable from `start` along the recorded ancestry. -/
theorem corr (g : ℕ → List ℕ) (start : ℕ) : spec g start (sol g start) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P1719
