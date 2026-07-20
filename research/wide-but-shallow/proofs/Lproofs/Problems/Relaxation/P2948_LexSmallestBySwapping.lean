import Lproofs.Generators

/-! @lc 2948 | name:Make Lexicographically Smallest Array by Swapping Elements | scheme:relaxation |
    family:union-find | complexity:O(n log n) |
    source:https://leetcode.com/problems/make-lexicographically-smallest-array-by-swapping-elements/

    Two elements may be swapped (repeatedly) when their values differ by at most `limit`; the accepted
    solution groups values into connected components under this relation and sorts within each. The
    component of a value is exactly its reachable set under the concrete swap relation. CLASSIFICATION
    (relaxation): the component is the least fixpoint of one-step relaxation of `swapRel`. CORRECTNESS
    (the component structure, not the final sort): the relaxation fixpoint is exactly the set of values
    reachable from a start value by a chain of within-`limit` swaps---the swap-equivalence class. -/

namespace LC.P2948
open Interview

/-- Concrete swap relation: two values are directly swappable when they differ by at most `limit`.
    This is the actual problem condition, not a free relation. -/
def swapRel (limit : ℤ) (a b : ℤ) : Prop := |a - b| ≤ limit

/-- The component of a start value: the values reachable from it under `swapRel`, as the least
    fixpoint of one-step relaxation. -/
def sol (limit start : ℤ) : Set ℤ := OrderHom.lfp (reachOp {start} (swapRel limit))

/-- Spec: the component is exactly the values reachable from `start` by a chain of within-`limit` swaps. -/
def spec (limit start : ℤ) (S : Set ℤ) : Prop :=
  S = {v | Relation.ReflTransGen (swapRel limit) start v}

/-- SCHEME (relaxation): the component is a fixpoint of one-step relaxation of the concrete swap relation. -/
theorem cls (limit start : ℤ) :
    reachOp {start} (swapRel limit) (sol limit start) = sol limit start :=
  reach_is_dp_fixpoint {start} (swapRel limit)

/-- CORRECT: the relaxation fixpoint is exactly the swap-equivalence class of `start`---the values
    reachable from it by within-`limit` swaps---which is the component the accepted solution sorts. -/
theorem corr (limit start : ℤ) : spec limit start (sol limit start) := by
  unfold spec sol
  rw [lfp_reachOp_eq_reachable]
  ext v
  simp [Set.mem_singleton_iff]


/-- TEST VECTOR: with limit 3, value 7 is in the swap component of 1 (chain 1→4→7 in steps of
    ≤ 3); with limit 0 nothing but 1 itself is (each step must be an exact-equal swap). -/
theorem vec : (7 : ℤ) ∈ sol 3 1 ∧ (2 : ℤ) ∉ sol 0 1 := by
  constructor
  · have h := corr 3 1
    unfold spec at h
    rw [h]
    simp only [Set.mem_setOf_eq]
    exact .head (show swapRel 3 1 4 by unfold swapRel; decide)
      (.head (show swapRel 3 4 7 by unfold swapRel; decide) .refl)
  · have h := corr 0 1
    unfold spec at h
    rw [h]
    simp only [Set.mem_setOf_eq]
    intro hr
    have fixed : ∀ v, Relation.ReflTransGen (swapRel 0) 1 v → v = 1 := by
      intro v hv
      induction hv with
      | refl => rfl
      | @tail b c _ step ih =>
        subst ih
        simp only [swapRel] at step
        rw [abs_le] at step
        omega
    exact absurd (fixed 2 hr) (by omega)

end LC.P2948
