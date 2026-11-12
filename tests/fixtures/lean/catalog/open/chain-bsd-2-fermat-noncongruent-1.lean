import Mathlib.Data.Nat.Basic

/-- A positive integer `n` is a congruent number if `ab = 2nd²` and `a² + b² = c²`
    for positive integers `a, b, c, d`. -/
def isCongruentNumber (n : ℕ) : Prop :=
  ∃ (a b c d : ℕ), 0 < a ∧ 0 < b ∧ 0 < c ∧ 0 < d ∧
    a ^ 2 + b ^ 2 = c ^ 2 ∧ a * b = 2 * n * (d ^ 2)

/-- Chain B [Rung 2/4 → BSD Congruent Numbers] (Fermat's Right-Triangle Theorem):
    `1` is not a congruent number (`¬ isCongruentNumber 1`), proved classically by infinite
    descent on `x⁴ - y⁴ = z²`. Via Rung B1, this establishes Mordell–Weil rank 0 on
    `E_1 : y² = x³ - x` where `L(E_1, 1) ≠ 0` (the rank-0 base case of BSD). -/
def statementProp : Prop :=
  ¬ isCongruentNumber 1
