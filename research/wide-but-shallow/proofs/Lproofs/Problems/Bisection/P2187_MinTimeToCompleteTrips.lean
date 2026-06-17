import Lproofs.Schemes.Bisect

/-! @lc 2187 | name:Minimum Time to Complete Trips | scheme:bisection | family:binary-search |
    complexity:O(n log T) | source:https://leetcode.com/problems/minimum-time-to-complete-trips/

    Each bus `i` takes `times[i]` per trip, so by time `T` it completes `⌊T/times[i]⌋` trips and the
    total `tripsBy T = Σ ⌊T/times[i]⌋` is monotone non-decreasing in `T`. Hence "at least `target`
    trips done by time `T`" is a *threshold* predicate, and the accepted solution binary-searches the
    least such `T`. CLASSIFICATION ONLY: we certify the bisection structure (the real predicate is a
    monotone threshold, so the answer is its `Nat.find`); we do not separately re-derive that this is
    the globally minimal time beyond it being the least feasible `T`. -/

namespace LC.P2187
open Interview.Patterns

/-- Trips completed across all buses by time `T` (the real per-bus computation). -/
def tripsBy (times : List ℕ) (T : ℕ) : ℕ := (times.map (fun t => T / t)).sum

/-- `completable T` = at least `target` trips are done by time `T`. -/
def completable (times : List ℕ) (target T : ℕ) : Prop := target ≤ tripsBy times T

instance (times : List ℕ) (target : ℕ) : DecidablePred (completable times target) :=
  fun T => inferInstanceAs (Decidable (target ≤ tripsBy times T))

/-- The accepted binary-search answer: the least time at which `target` trips are done. -/
def sol (times : List ℕ) (target : ℕ) (h : ∃ T, completable times target T) : ℕ := Nat.find h

/-- `tripsBy` is monotone in `T`: more time gives at least as many trips (real, non-vacuous). -/
theorem tripsBy_mono (times : List ℕ) {a b : ℕ} (hab : a ≤ b) : tripsBy times a ≤ tripsBy times b := by
  induction times with
  | nil => simp [tripsBy]
  | cons t ts ih =>
    simp only [tripsBy, List.map_cons, List.sum_cons]
    exact Nat.add_le_add (Nat.div_le_div_right hab) ih

/-- `completable` inherits monotonicity — the up-set / threshold structure of bisection. -/
theorem completable_mono (times : List ℕ) (target : ℕ) :
    ∀ a b, a ≤ b → completable times target a → completable times target b :=
  fun _ _ hab ha => le_trans ha (tripsBy_mono times hab)

/-- SCHEME (bisection): the real trip-count predicate is a monotone threshold —
    `completable n ↔ (least feasible time) ≤ n` — which is exactly what binary search exploits. -/
theorem cls (times : List ℕ) (target : ℕ) (h : ∃ T, completable times target T) (n : ℕ) :
    completable times target n ↔ sol times target h ≤ n :=
  bisection_threshold (completable times target) (completable_mono times target) h n

/-- The answer is the least feasible time (the threshold). -/
theorem corr (times : List ℕ) (target : ℕ) (h : ∃ T, completable times target T) :
    IsLeast {T | completable times target T} (sol times target h) :=
  bisection_isLeast (completable times target) h

end LC.P2187
