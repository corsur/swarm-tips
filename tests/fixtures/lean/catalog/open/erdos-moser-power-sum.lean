import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import Mathlib.Order.Interval.Finset.Nat

/-!
# Erdős Open Problem: The Erdős–Moser Power-Sum Conjecture (1953)

Paul Erdős conjectured in a 1953 letter to Leo Moser that the Diophantine equation
$$\sum_{k=1}^{m-1} k^n = m^n \qquad (n \ge 1,\ m \ge 2)$$
has only the single solution $(m, n) = (3, 1)$, corresponding to $1^1 + 2^1 = 3^1$.

Moser (1953) proved any other solution must satisfy $m > 10^{10^6}$, and subsequent
computational work pushed the lower bound past $m > 10^{10^9}$, yet the conjecture remains
open. In the post-RH catalog, it represents a clean, self-contained Diophantine frontier
problem with zero edge-case ambiguity (`1 ≤ n` rules out the degenerate `n = 0, m = 2` case,
and `2 ≤ m` rules out the empty sum at `m = 0`).
-/

/-- Erdős–Moser Conjecture: for integers `n ≥ 1` and `m ≥ 2`, the only solution to
    `∑_{k=1}^{m-1} k^n = m^n` is `m = 3` and `n = 1`. -/
def statementProp : Prop :=
  ∀ (m n : ℕ), 1 ≤ n → 2 ≤ m →
    Finset.sum (Finset.Ico 1 m) (fun k => k ^ n) = m ^ n →
    m = 3 ∧ n = 1
