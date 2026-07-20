import Lproofs.Schemes.Bisect

/-! @lc 278 | name:First Bad Version | scheme:bisection | family:binary-search | complexity:O(log n) |
    source:https://leetcode.com/problems/first-bad-version/

    The input is the `isBadVersion` oracle: a concrete predicate `isBad : ℕ → Bool` on versions that is
    monotone — once a version is bad, every later version is bad. Binary-search the boundary.
    CLASSIFICATION: `bad isBad` is a monotone threshold, so the search returns its least satisfying
    index. `cls` certifies the threshold structure of the concrete oracle; `corr` that the answer is the
    least bad version of `isBad`. -/

namespace LC.P0278
open Interview.Patterns

/-- The concrete `isBadVersion` oracle reports each version bad/good. -/
def bad (isBad : ℕ → Bool) (n : ℕ) : Prop := isBad n = true

instance (isBad : ℕ → Bool) : DecidablePred (bad isBad) :=
  fun n => inferInstanceAs (Decidable (isBad n = true))

/-- The binary-search answer: the first bad version (the decision boundary). -/
def sol (isBad : ℕ → Bool) (h : ∃ n, bad isBad n) : ℕ := Nat.find h

/-- Spec: the answer is the least bad version of the oracle. -/
def spec (isBad : ℕ → Bool) (n : ℕ) : Prop := IsLeast {m | bad isBad m} n

/-- SCHEME (bisection): the monotone oracle is a threshold — `bad isBad n ↔ answer ≤ n`. -/
theorem cls (isBad : ℕ → Bool) (mono : ∀ a b, a ≤ b → bad isBad a → bad isBad b)
    (h : ∃ n, bad isBad n) (n : ℕ) : bad isBad n ↔ sol isBad h ≤ n :=
  bisection_threshold (bad isBad) mono h n

/-- CORRECT: the binary-search answer is the first (least) bad version of the concrete oracle. -/
theorem corr (isBad : ℕ → Bool) (h : ∃ n, bad isBad n) : spec isBad (sol isBad h) :=
  bisection_isLeast (bad isBad) h


/-- TEST VECTOR (official example): n = 5 with versions ≥ 4 bad — the search returns 4. -/
theorem vec : sol (fun n => decide (4 ≤ n)) ⟨4, by decide⟩ = 4 := by
  simp only [sol]
  rw [Nat.find_eq_iff]
  decide

end LC.P0278
