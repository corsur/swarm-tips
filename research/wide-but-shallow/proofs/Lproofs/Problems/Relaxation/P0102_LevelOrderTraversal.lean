import Lproofs.Generators

/-! @lc 102 | name:Binary Tree Level Order Traversal | scheme:relaxation | family:bfs | complexity:O(n) |
    source:https://leetcode.com/problems/binary-tree-level-order-traversal/

    Level-order BFS visits every node reachable from the root by repeatedly stepping to children.
    CLASSIFICATION: the set of visited nodes is the least fixpoint of one-step relaxation under the
    concrete child relation `childRel kids` (`v` is a child of `u` per the adjacency `kids`). `cls`
    certifies the relaxation fixpoint; `corr` proves the visited set is exactly the nodes reachable
    from the root through child links — over a concrete relation, not a free `child`. (The level
    grouping orders this set by depth.) -/

namespace LC.P0102
open Interview

/-- Concrete child relation: `v` is a child of `u` per the children adjacency `kids`. -/
def childRel (kids : ℕ → List ℕ) (u v : ℕ) : Prop := v ∈ kids u

/-- The visited set: nodes reachable from `root` under `childRel`, the least relaxation fixpoint. -/
def sol (kids : ℕ → List ℕ) (root : ℕ) : Set ℕ := OrderHom.lfp (reachOp {root} (childRel kids))

/-- Spec: exactly the nodes reachable from `root` along child links. -/
def spec (kids : ℕ → List ℕ) (root : ℕ) (S : Set ℕ) : Prop :=
  S = {v | Relation.ReflTransGen (childRel kids) root v}

/-- SCHEME (relaxation): the visited set is a fixpoint of one-step relaxation of the child relation. -/
theorem cls (kids : ℕ → List ℕ) (root : ℕ) :
    reachOp {root} (childRel kids) (sol kids root) = sol kids root :=
  reach_is_dp_fixpoint {root} (childRel kids)

/-- CORRECT: the visited set is exactly the nodes reachable from `root` through child links. -/
theorem corr (kids : ℕ → List ℕ) (root : ℕ) : spec kids root (sol kids root) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]

end LC.P0102
