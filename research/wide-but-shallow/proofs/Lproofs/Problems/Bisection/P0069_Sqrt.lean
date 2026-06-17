import Lproofs.Schemes.Bisect

/-! @lc 69 | name:Sqrt(x) | scheme:bisection | family:binary-search | complexity:O(log x) |
    source:https://leetcode.com/problems/sqrtx/ -/

namespace LC.P0069

/-- Spec: the integer square root — the greatest `k` with `k² ≤ x` (`k² ≤ x < (k+1)²`). -/
def spec (x k : ℕ) : Prop := k * k ≤ x ∧ x < (k + 1) * (k + 1)

/-- Editorial binary search on the monotone predicate `k² > x`, i.e. the integer sqrt. -/
def sol (x : ℕ) : ℕ := Nat.sqrt x

/-- SCHEME (bisection): "too big" (`x < k²`) is monotone in `k` — the up-set/threshold structure
    that makes binary search on the boundary correct. -/
theorem cls (x : ℕ) : ∀ a b : ℕ, a ≤ b → x < a * a → x < b * b :=
  fun _ _ hab h => lt_of_lt_of_le h (Nat.mul_le_mul hab hab)

/-- CORRECT: `sol` is the integer square root (both threshold bounds hold). -/
theorem corr (x : ℕ) : spec x (sol x) :=
  ⟨Nat.sqrt_le x, Nat.lt_succ_sqrt x⟩

end LC.P0069
