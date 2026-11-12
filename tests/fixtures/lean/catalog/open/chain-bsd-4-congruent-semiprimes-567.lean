import Mathlib.Data.Nat.Prime.Basic

/-- A positive integer `n` is a congruent number if `ab = 2nd²` and `a² + b² = c²`
    for positive integers `a, b, c, d`. -/
def isCongruentNumber (n : ℕ) : Prop :=
  ∃ (a b c d : ℕ), 0 < a ∧ 0 < b ∧ 0 < c ∧ 0 < d ∧
    a ^ 2 + b ^ 2 = c ^ 2 ∧ a * b = 2 * n * (d ^ 2)

/-- Chain B [Rung 4/4 → BSD Congruent Numbers] (Open Semiprime BSD Slice, ω(n) = 2):
    Every semiprime `n = p · q` (`p < q` prime) with `(p · q) ≡ 5, 6, 7 (mod 8)` is a
    congruent number. Isolates the exact first open Selmer-rank ≥ 3 frontier (`p ≡ 1 mod 8`,
    `q ≡ 5 mod 8`, Legendre symbol `(p/q) = +1`) of `millennium-bsd-congruent-squarefree`. -/
def statementProp : Prop :=
  ∀ (p q : ℕ), Nat.Prime p → Nat.Prime q → p < q →
    ((p * q) % 8 = 5 ∨ (p * q) % 8 = 6 ∨ (p * q) % 8 = 7) →
      isCongruentNumber (p * q)
