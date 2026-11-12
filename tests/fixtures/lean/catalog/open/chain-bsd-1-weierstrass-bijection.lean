import Mathlib.Data.Nat.Basic
import Mathlib.Data.Rat.Defs
import Mathlib.Tactic.Ring

/-- A positive integer `n` is a congruent number if `ab = 2nd²` and `a² + b² = c²`
    for positive integers `a, b, c, d` (representing rational sides `a/d, b/d, c/d`). -/
def isCongruentNumber (n : ℕ) : Prop :=
  ∃ (a b c d : ℕ), 0 < a ∧ 0 < b ∧ 0 < c ∧ 0 < d ∧
    a ^ 2 + b ^ 2 = c ^ 2 ∧ a * b = 2 * n * (d ^ 2)

/-- Chain B [Rung 1/4 → BSD Congruent Numbers] (Koblitz Prop. I.2 Weierstrass Bijection):
    For every integer `n ≥ 1`, `n` is the area of a rational right triangle (`isCongruentNumber n`)
    if and only if the congruent-number elliptic curve `E_n : y² = x³ - n²x` admits a rational
    point `(x, y) ∈ ℚ × ℚ` outside the 2-torsion locus `y = 0`. -/
def statementProp : Prop :=
  ∀ (n : ℕ), 1 ≤ n →
    (isCongruentNumber n ↔ ∃ (x y : ℚ), y ≠ 0 ∧ y ^ 2 = x ^ 3 - (n : ℚ) ^ 2 * x)
