import Lproofs.Schemes.Bisect

/-! @lc 981 | name:Time Based Key-Value Store | scheme:bisection | family:binary-search |
    complexity:O(log n) | source:https://leetcode.com/problems/time-based-key-value-store/

    `get(key, t)` returns the value with the largest timestamp ≤ `t`; timestamps are stored sorted, so
    a binary search locates it. CLASSIFICATION: the predicate `tooLate i` ("stored timestamp `i`
    exceeds `t`") is a monotone up-set (timestamps increase), so the binary search finds its least
    satisfying index — the floor sits just before it. `cls` certifies the threshold structure;
    `corr` that the answer is the least such index. We certify the monotone-threshold structure. -/

namespace LC.P0981
open Interview.Patterns

/-- `tooLate i` — the `i`-th stored timestamp is past the query `t` (timestamps are sorted). -/
def tooLate (times : ℕ → ℕ) (t i : ℕ) : Prop := t < times i

instance (times : ℕ → ℕ) (t : ℕ) : DecidablePred (tooLate times t) :=
  fun i => inferInstanceAs (Decidable (t < times i))

/-- The binary-search boundary: the least index whose timestamp exceeds the query. -/
def sol (times : ℕ → ℕ) (t : ℕ) (h : ∃ i, tooLate times t i) : ℕ := Nat.find h

/-- `tooLate` is a monotone up-set when timestamps are sorted (monotone): once past `t`, it stays. -/
theorem tooLate_mono (times : ℕ → ℕ) (hmono : Monotone times) (t : ℕ) :
    ∀ a b, a ≤ b → tooLate times t a → tooLate times t b :=
  fun a b hab ha => lt_of_lt_of_le ha (hmono hab)

/-- SCHEME (bisection): the predicate is a monotone threshold — `tooLate i ↔ boundary ≤ i`. -/
theorem cls (times : ℕ → ℕ) (hmono : Monotone times) (t : ℕ) (h : ∃ i, tooLate times t i) (n : ℕ) :
    tooLate times t n ↔ sol times t h ≤ n :=
  bisection_threshold (tooLate times t) (tooLate_mono times hmono t) h n

/-- The boundary is the least past-the-query index (the floor query reads just before it). -/
theorem corr (times : ℕ → ℕ) (t : ℕ) (h : ∃ i, tooLate times t i) :
    IsLeast {i | tooLate times t i} (sol times t h) :=
  bisection_isLeast (tooLate times t) h

end LC.P0981
