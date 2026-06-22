import Lproofs.Schemes.Fold

/-! @lc 1387 | name:Sort Integers by The Power Value | scheme:fold | family:math-bit |
    complexity:O(n log n · steps) | source:https://leetcode.com/problems/sort-integers-by-the-power-value/

    The "power" of `x` is the number of Collatz steps (`x/2` if even, `3x+1` if odd) to reach 1; the
    answer sorts the range by `(power, value)`. CLASSIFICATION (fold): each power value is a streaming
    recursion (the Collatz iteration), and the answer is a sort by that key. CORRECTNESS (a
    problem-specific property of the power function, not Collatz's open termination): we certify that the
    power of a power of two is exactly its exponent --- `power (2^k) = k` --- since `2^k` halves to `1`
    in exactly `k` steps. -/

namespace LC.P1387

/-- One Collatz step. -/
def step (n : ℕ) : ℕ := if n % 2 = 0 then n / 2 else 3 * n + 1

/-- The power value: Collatz steps to reach 1, with `fuel` bounding the recursion. -/
def power : ℕ → ℕ → ℕ
  | 0, _ => 0
  | f + 1, n => if n = 1 then 0 else 1 + power f (step n)

/-- SCHEME (fold): the power value is a streaming recursion --- one Collatz step then recurse. -/
theorem cls (f n : ℕ) :
    power (f + 1) n = if n = 1 then 0 else 1 + power f (step n) := rfl

theorem power_one (f : ℕ) : power f 1 = 0 := by
  cases f with
  | zero => rfl
  | succ f => simp [power]

/-- CORRECT: the power of `2^k` is exactly `k` --- it halves to 1 in `k` steps. A genuine
    problem-specific Collatz fact (not the open general termination). -/
theorem corr (k f : ℕ) (hf : k ≤ f) : power f (2 ^ k) = k := by
  induction k generalizing f with
  | zero => simpa using power_one f
  | succ k ih =>
    obtain ⟨f, rfl⟩ : ∃ g, f = g + 1 := ⟨f - 1, by omega⟩
    have hne : (2 : ℕ) ^ (k + 1) ≠ 1 := by
      have : (2 : ℕ) ≤ 2 ^ (k + 1) := by
        calc (2 : ℕ) = 2 ^ 1 := (pow_one 2).symm
          _ ≤ 2 ^ (k + 1) := Nat.pow_le_pow_right (by norm_num) (by omega)
      omega
    have hps : (2 : ℕ) ^ (k + 1) = 2 ^ k * 2 := pow_succ 2 k
    have heven : (2 : ℕ) ^ (k + 1) % 2 = 0 := by omega
    have hstep : step (2 ^ (k + 1)) = 2 ^ k := by
      simp only [step, heven, if_pos]; omega
    rw [power, if_neg hne, hstep, ih f (by omega)]
    omega

end LC.P1387