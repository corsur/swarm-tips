-- swarm:polarity=bidirectional
import Mathlib.Data.Nat.Factorial.Basic

/-!
# Bidirectional Open Problem (Prove OR Disprove): Brocard's Problem (1876)

Henri Brocard (1876, 1885) and independently Srinivasa Ramanujan (1913) asked for all
non-negative integer solutions `(n, m)` to the factorial-square equation
$$n! + 1 = m^2.$$
The only known solutions ("Brown numbers") are `(4, 5)`, `(5, 11)`, and `(7, 71)`:
`4! + 1 = 25 = 5^2`, `5! + 1 = 121 = 11^2`, and `7! + 1 = 5041 = 71^2`. No further solution
exists for `n ≤ 10^9`, and Overholt (1993) showed finiteness follows from the ABC conjecture,
but whether a fourth Brown number exists remains completely open.

Because this campaign carries `-- swarm:polarity=bidirectional`, a solver wins either by
proving `theorem proof : statementProp` (no larger solutions exist) or by proving
`theorem disproof : ¬ statementProp` (exhibiting a valid counterexample pair `(n, m)` with
`n > 7`).
-/

/-- Brocard's Problem: every natural-number solution to `n! + 1 = m^2` has `n ∈ {4, 5, 7}`.
    Bidirectional bounty: prove `statementProp` or refute it via `¬ statementProp`. -/
def statementProp : Prop :=
  ∀ (n m : ℕ), Nat.factorial n + 1 = m ^ 2 →
    n = 4 ∨ n = 5 ∨ n = 7
