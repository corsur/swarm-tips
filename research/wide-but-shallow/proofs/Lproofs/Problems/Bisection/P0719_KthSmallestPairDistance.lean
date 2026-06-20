import Lproofs.Schemes.Bisect

/-! @lc 719 | name:Find K-th Smallest Pair Distance | scheme:bisection | family:binary-search |
    complexity:O(n log n + n log W) | source:https://leetcode.com/problems/find-k-th-smallest-pair-distance/

    The k-th smallest absolute distance among all index pairs is found by binary-searching `d`: the
    number of pairs at distance `≤ d` is non-decreasing in `d`, so the answer is the least `d` admitting
    at least `k` such pairs. CLASSIFICATION (bisection): `feasible d` ("at least `k` pairs lie within
    `d`") is a monotone up-set in `d` — proven concretely from the pair-distance multiset, not assumed
    — so the binary search returns its least satisfying value. `cls` certifies the threshold structure;
    `corr` that `sol` is that least feasible distance, i.e. the k-th smallest pair distance. -/

namespace LC.P0719
open Interview.Patterns

/-- All pair distances `nums j - nums i` for `i < j < m` (the concrete multiset binary-searched). -/
def pairDists (nums : ℕ → ℕ) (m : ℕ) : List ℕ :=
  (List.range m).flatMap fun j => (List.range j).map fun i => nums j - nums i

/-- Number of index pairs whose distance is at most `d`. -/
def pairsWithin (nums : ℕ → ℕ) (m d : ℕ) : ℕ :=
  (pairDists nums m).countP fun x => decide (x ≤ d)

/-- Feasibility the binary search tests: at least `k` pairs lie within distance `d`. -/
def feasible (nums : ℕ → ℕ) (m k d : ℕ) : Prop := k ≤ pairsWithin nums m d

instance (nums : ℕ → ℕ) (m k : ℕ) : DecidablePred (feasible nums m k) :=
  fun d => inferInstanceAs (Decidable (k ≤ pairsWithin nums m d))

/-- The binary-search answer: the least distance admitting at least `k` pairs. -/
def sol (nums : ℕ → ℕ) (m k : ℕ) (h : ∃ d, feasible nums m k d) : ℕ := Nat.find h

/-- The within-`d` pair count is monotone in `d` (a wider radius keeps every pair). -/
theorem pairsWithin_mono (nums : ℕ → ℕ) (m : ℕ) : Monotone (pairsWithin nums m) := by
  intro a b hab
  apply List.countP_mono_left
  intro x _ hx
  simp only [decide_eq_true_eq] at hx ⊢
  exact le_trans hx hab

theorem feasible_mono (nums : ℕ → ℕ) (m k : ℕ) :
    ∀ a b, a ≤ b → feasible nums m k a → feasible nums m k b :=
  fun _ _ hab ha => le_trans ha (pairsWithin_mono nums m hab)

/-- SCHEME (bisection): feasibility is a monotone threshold — `feasible d ↔ (k-th distance) ≤ d`. -/
theorem cls (nums : ℕ → ℕ) (m k : ℕ) (h : ∃ d, feasible nums m k d) (n : ℕ) :
    feasible nums m k n ↔ sol nums m k h ≤ n :=
  bisection_threshold (feasible nums m k) (feasible_mono nums m k) h n

/-- CORRECT: `sol` is the least distance with at least `k` pairs within it — the k-th smallest pair
    distance over the concrete distance multiset. -/
theorem corr (nums : ℕ → ℕ) (m k : ℕ) (h : ∃ d, feasible nums m k d) :
    IsLeast {d | feasible nums m k d} (sol nums m k h) :=
  bisection_isLeast (feasible nums m k) h

end LC.P0719
