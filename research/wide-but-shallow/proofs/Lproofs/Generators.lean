import Mathlib

/-!
# Machine-checked core of the "generators" claim

Two of the interview-problem *generators* are genuinely algebraic; this file certifies
the two cleanest instances in Lean + Mathlib.

* `range_fold` — the **hash / running-state** generator. The "small modification"
  monoid → group (adding inverses) turns prefix-only folds into arbitrary O(1) range
  folds. This is prefix-sum / difference-array, stated for any group.

* `agg_hom` — the **recursive-fold / DP-over-a-semiring** generator (semiring
  polymorphism). One generic aggregation `agg` ("sum over paths of product of weights")
  is transported across *any* semiring homomorphism. Instantiating the semiring `R`
  recovers different classical algorithms from the *same* definition — the formal
  content of "swap the algebra in a fixed recursion scheme".
-/

namespace Interview

/-! ## Generator: running-state (prefix folds), monoid → group -/

/-- Prefix fold of the first `k` elements. In a monoid you can fold only prefixes. -/
def pre {M : Type*} [Monoid M] (a : List M) (k : ℕ) : M := (a.take k).prod

@[simp] theorem pre_def {M : Type*} [Monoid M] (a : List M) (k : ℕ) :
    pre a k = (a.take k).prod := rfl

/-- In a **group**, any contiguous range fold equals `(pre a i)⁻¹ * pre a j`.
    Adding inverses (monoid → group) is the one-component modification that upgrades
    prefix-only folds to arbitrary O(1) range folds (prefix-sum / difference-array). -/
theorem range_fold {G : Type*} [Group G] (a : List G) {i j : ℕ} (h : i ≤ j) :
    (pre a i)⁻¹ * pre a j = ((a.drop i).take (j - i)).prod := by
  obtain ⟨k, rfl⟩ : ∃ k, j = i + k := ⟨j - i, by omega⟩
  simp only [pre_def, Nat.add_sub_cancel_left, List.take_add, List.prod_append,
    inv_mul_cancel_left]

/-! ## Generator: recursive-fold / DP over a semiring (semiring polymorphism) -/

/-- Sum-over-paths of product-of-weights: one generic recurrence over any semiring.
    `R = ℕ`  → counting · `R = Tropical` → shortest path · `R = Bool/Prop` → reachability. -/
def agg {R : Type*} [CommSemiring R] (paths : List (List R)) : R :=
  (paths.map fun p => p.prod).sum

/-- **Transport law / semiring polymorphism.** `agg` commutes with every semiring
    homomorphism `f : R →+* S`. Holding the recursion scheme fixed and swapping the
    algebra (via `f`) is sound — the precise sense in which one algorithm "becomes"
    another by a small modification. -/
theorem agg_hom {R S : Type*} [CommSemiring R] [CommSemiring S]
    (f : R →+* S) (paths : List (List R)) :
    f (agg paths) = agg (paths.map fun p => p.map f) := by
  induction paths with
  | nil => simp [agg]
  | cons p ps ih =>
    simp only [agg, List.map_cons, List.sum_cons, map_add] at *
    rw [ih, map_list_prod]

/-! ### Payoff: the SAME `agg`, instantiated at different semirings. -/

-- Over `(ℕ, +, ×)` with unit weights: `agg` counts the paths.
example : agg ([[1, 1], [1], [1, 1, 1]] : List (List ℕ)) = 3 := by decide

-- The transport law in action: pushing a count through the `ℕ → ℝ` semiring hom.
example (paths : List (List ℕ)) :
    ((agg paths : ℕ) : ℝ) = agg (paths.map fun p => p.map (Nat.cast : ℕ → ℝ)) :=
  agg_hom (Nat.castRingHom ℝ) paths

/-! ## Generator: relaxation / frontier as a least fixpoint (the DP characterization)

The relaxation generator (BFS, Dijkstra-style search, Bellman--Ford, topological DP)
computes the **least fixpoint of a monotone "Bellman" operator** on a complete lattice.
This is the precise sense in which relaxation *is* a dynamic program (Knaster--Tarski /
Bellman optimality), and exactly what binary search and greedy do not share: there is no
monotone operator whose least fixpoint over overlapping subproblems they compute. -/

section Relaxation
variable {L : Type*} [CompleteLattice L]

/-- The DP/relaxation solution `lfp B` is a genuine fixpoint: relaxation has converged. -/
theorem bellman_fixpoint (B : L →o L) : B (OrderHom.lfp B) = OrderHom.lfp B :=
  OrderHom.map_lfp B

/-- It is the **least** relaxation-stable value: any `x` already stable under one round of
    relaxation (`B x ≤ x`) dominates it. (Bellman optimality; no over-relaxation.) -/
theorem bellman_least (B : L →o L) {x : L} (h : B x ≤ x) : OrderHom.lfp B ≤ x :=
  OrderHom.lfp_le B h

/-- Equivalently: the DP solution is the least fixpoint of the Bellman operator. -/
theorem bellman_isLeast (B : L →o L) :
    IsLeast (Function.fixedPoints B) (OrderHom.lfp B) :=
  OrderHom.isLeast_lfp B

/-- A concrete relaxation operator: one round of reachability from sources `s` along edge
    relation `r`. Monotone, hence its iterate has a least fixpoint = the reachable set. -/
def reachOp {V : Type*} (s : Set V) (r : V → V → Prop) : Set V →o Set V where
  toFun S := s ∪ {v | ∃ u ∈ S, r u v}
  monotone' _ _ hAB := by
    apply Set.union_subset_union_right
    rintro v ⟨u, hu, hr⟩
    exact ⟨u, hAB hu, hr⟩

/-- The reachable set is exactly the least-fixpoint DP solution of one-step relaxation. -/
theorem reach_is_dp_fixpoint {V : Type*} (s : Set V) (r : V → V → Prop) :
    reachOp s r (OrderHom.lfp (reachOp s r)) = OrderHom.lfp (reachOp s r) :=
  bellman_fixpoint _

/-- **The relaxation lfp is the genuine reachable set.** The least fixpoint of one-step relaxation
    equals the set of vertices reachable from the sources `s` along `r` — i.e. Mathlib's
    reflexive-transitive closure `Relation.ReflTransGen`. This ties the abstract DP fixpoint to the
    concrete graph answer (BFS/DFS/topological reachability), so a relaxation cert classifies the
    real computation, not a placeholder. -/
theorem lfp_reachOp_eq_reachable {V : Type*} (s : Set V) (r : V → V → Prop) :
    OrderHom.lfp (reachOp s r) = {v | ∃ u ∈ s, Relation.ReflTransGen r u v} := by
  apply le_antisymm
  · -- lfp ≤ reachable set: the reachable set is a prefixpoint of `reachOp`.
    apply OrderHom.lfp_le
    intro v hv
    simp only [reachOp, OrderHom.coe_mk, Set.mem_union, Set.mem_setOf_eq] at hv
    rcases hv with hv | ⟨u, ⟨w, hw, hwu⟩, hr⟩
    · exact ⟨v, hv, Relation.ReflTransGen.refl⟩
    · exact ⟨w, hw, hwu.tail hr⟩
  · -- reachable set ≤ lfp: induct on the closure, using that lfp is a fixpoint.
    rintro v ⟨u, hu, huv⟩
    have hfix := OrderHom.map_lfp (reachOp s r)
    induction huv with
    | refl =>
      rw [← hfix]; exact Set.mem_union_left _ hu
    | tail _ hr ih =>
      rw [← hfix]; exact Set.mem_union_right _ ⟨_, ih, hr⟩

/-- Reachability from `src` in exactly `n` steps along `r` (the length-counted closure). -/
inductive Reaches {V : Type*} (r : V → V → Prop) (src : V) : V → ℕ → Prop
  | refl : Reaches r src src 0
  | step {u v n} : Reaches r src u n → r u v → Reaches r src v (n + 1)

/-- **Bounded relaxation = bounded reachability (BFS layers).** Iterating one-step relaxation from
    the single source `src` exactly `n+1` times yields precisely the vertices reachable in at most
    `n` steps. Hence the BFS/shortest-hop distance is the least `n` with `v ∈ (reachOp {src} r)^[n+1]
    ∅` — tying the relaxation scheme to the concrete graph *distance*, not just reachability. -/
theorem mem_reachOp_iterate {V : Type*} (src : V) (r : V → V → Prop) :
    ∀ (n : ℕ) (v : V), v ∈ (reachOp ({src} : Set V) r)^[n + 1] (∅ : Set V) ↔
      ∃ m ≤ n, Reaches r src v m := by
  intro n
  induction n with
  | zero =>
    intro v
    rw [Function.iterate_one]
    simp only [reachOp, OrderHom.coe_mk, Set.mem_union, Set.mem_setOf_eq, Set.mem_empty_iff_false,
      false_and, exists_false, or_false, Set.mem_singleton_iff]
    constructor
    · rintro rfl; exact ⟨0, le_refl _, Reaches.refl⟩
    · rintro ⟨m, hm, hr⟩; interval_cases m; cases hr; rfl
  | succ n ih =>
    intro v
    rw [Function.iterate_succ', Function.comp_apply]
    simp only [reachOp, OrderHom.coe_mk, Set.mem_union, Set.mem_setOf_eq, Set.mem_singleton_iff]
    constructor
    · rintro (rfl | ⟨u, hu, hr⟩)
      · exact ⟨0, Nat.zero_le _, Reaches.refl⟩
      · obtain ⟨m, hm, hru⟩ := (ih u).mp hu
        exact ⟨m + 1, by omega, Reaches.step hru hr⟩
    · rintro ⟨m, hm, hr⟩
      cases hr with
      | refl => exact Or.inl rfl
      | @step u _ k hru hr =>
        exact Or.inr ⟨u, (ih u).mpr ⟨k, by omega, hru⟩, hr⟩

/-- Multi-source reachability: reachable from *some* vertex of `s` in exactly `n` steps. -/
inductive ReachesS {V : Type*} (r : V → V → Prop) (s : Set V) : V → ℕ → Prop
  | base {v} : v ∈ s → ReachesS r s v 0
  | step {u v n} : ReachesS r s u n → r u v → ReachesS r s v (n + 1)

/-- **Multi-source BFS layers.** Iterating relaxation from a *set* of sources `s` exactly `n+1`
    times yields the vertices reachable from `s` in at most `n` steps. So the multi-source BFS
    distance (e.g. distance to the nearest source) is the least such `n`. -/
theorem mem_reachOp_iterate_set {V : Type*} (s : Set V) (r : V → V → Prop) :
    ∀ (n : ℕ) (v : V), v ∈ (reachOp s r)^[n + 1] (∅ : Set V) ↔ ∃ m ≤ n, ReachesS r s v m := by
  intro n
  induction n with
  | zero =>
    intro v
    rw [Function.iterate_one]
    simp only [reachOp, OrderHom.coe_mk, Set.mem_union, Set.mem_setOf_eq, Set.mem_empty_iff_false,
      false_and, exists_false, or_false]
    constructor
    · intro hv; exact ⟨0, le_refl _, ReachesS.base hv⟩
    · rintro ⟨m, hm, hr⟩; interval_cases m; cases hr; assumption
  | succ n ih =>
    intro v
    rw [Function.iterate_succ', Function.comp_apply]
    simp only [reachOp, OrderHom.coe_mk, Set.mem_union, Set.mem_setOf_eq]
    constructor
    · rintro (hv | ⟨u, hu, hr⟩)
      · exact ⟨0, Nat.zero_le _, ReachesS.base hv⟩
      · obtain ⟨m, hm, hru⟩ := (ih u).mp hu
        exact ⟨m + 1, by omega, ReachesS.step hru hr⟩
    · rintro ⟨m, hm, hr⟩
      cases hr with
      | base hv => exact Or.inl hv
      | @step u _ k hru hr =>
        exact Or.inr ⟨u, (ih u).mpr ⟨k, by omega, hru⟩, hr⟩

end Relaxation

end Interview
