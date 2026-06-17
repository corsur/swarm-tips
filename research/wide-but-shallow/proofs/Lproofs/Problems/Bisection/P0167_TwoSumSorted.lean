import Lproofs.Schemes.Bisect

/-! @lc 167 | name:Two Sum II - Input Array Is Sorted | scheme:bisection | family:binary-search |
    complexity:O(n log n) | source:https://leetcode.com/problems/two-sum-ii-input-array-is-sorted/

    For each element, binary search the sorted array for its complement (`target − a`); this is an
    accepted O(n log n) solution. CLASSIFICATION: the predicate `atLeastComp i` ("the `i`-th value
    reaches the complement") is a monotone up-set (the array is sorted), so the binary search returns
    its least satisfying index. `cls` certifies the threshold structure; `corr` that the answer is the
    least such index. We certify the monotone-threshold structure the binary search exploits. -/

namespace LC.P0167
open Interview.Patterns

/-- `atLeastComp i` — the `i`-th value reaches the complement `comp` (sorted array `nums`). -/
def atLeastComp (nums : ℕ → ℤ) (comp : ℤ) (i : ℕ) : Prop := comp ≤ nums i

instance (nums : ℕ → ℤ) (comp : ℤ) : DecidablePred (atLeastComp nums comp) :=
  fun i => inferInstanceAs (Decidable (comp ≤ nums i))

/-- The binary-search answer: the least index whose value reaches the complement. -/
def sol (nums : ℕ → ℤ) (comp : ℤ) (h : ∃ i, atLeastComp nums comp i) : ℕ := Nat.find h

/-- `atLeastComp` is a monotone up-set when `nums` is sorted (monotone): once reached, it stays. -/
theorem comp_mono (nums : ℕ → ℤ) (hmono : Monotone nums) (comp : ℤ) :
    ∀ a b, a ≤ b → atLeastComp nums comp a → atLeastComp nums comp b :=
  fun a b hab ha => le_trans ha (hmono hab)

/-- SCHEME (bisection): the predicate is a monotone threshold — `atLeastComp i ↔ answer ≤ i`. -/
theorem cls (nums : ℕ → ℤ) (hmono : Monotone nums) (comp : ℤ) (h : ∃ i, atLeastComp nums comp i) (n : ℕ) :
    atLeastComp nums comp n ↔ sol nums comp h ≤ n :=
  bisection_threshold (atLeastComp nums comp) (comp_mono nums hmono comp) h n

/-- The answer is the least index reaching the complement — the binary-search target. -/
theorem corr (nums : ℕ → ℤ) (comp : ℤ) (h : ∃ i, atLeastComp nums comp i) :
    IsLeast {i | atLeastComp nums comp i} (sol nums comp h) :=
  bisection_isLeast (atLeastComp nums comp) h

end LC.P0167
