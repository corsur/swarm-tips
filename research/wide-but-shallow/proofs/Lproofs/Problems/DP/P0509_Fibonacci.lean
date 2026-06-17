import Lproofs.Schemes.Fold

/-! @lc 509 | name:Fibonacci Number | scheme:dp | family:dp-linear | complexity:O(n) |
    source:https://leetcode.com/problems/fibonacci-number/ -/

namespace LC.P0509

/-- Spec: the n-th Fibonacci number. -/
def spec (n : ℕ) : ℕ := Nat.fib n

/-- Editorial O(1)-space iterative DP: carry the last two values. -/
def iter : ℕ → ℕ × ℕ
  | 0 => (0, 1)
  | (n + 1) => ((iter n).2, (iter n).1 + (iter n).2)

def sol (n : ℕ) : ℕ := (iter n).1

/-- SCHEME (DP catamorphism): the iterative state is exactly `(fib n, fib (n+1))`. -/
theorem cls (n : ℕ) : iter n = (Nat.fib n, Nat.fib (n + 1)) := by
  induction n with
  | zero => rfl
  | succ k ih => rw [iter, ih]; simp [Nat.fib_add_two]

/-- CORRECT: the iterative solution equals the Fibonacci recurrence (the spec). -/
theorem corr (n : ℕ) : sol n = spec n := by rw [sol, spec, cls]

end LC.P0509
