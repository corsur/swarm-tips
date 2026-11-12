import Mathlib.Data.Nat.Prime.Basic

/-- A positive integer `n` is a congruent number if `ab = 2nd²` and `a² + b² = c²`
    for positive integers `a, b, c, d`. -/
def isCongruentNumber (n : ℕ) : Prop :=
  ∃ (a b c d : ℕ), 0 < a ∧ 0 < b ∧ 0 < c ∧ 0 < d ∧
    a ^ 2 + b ^ 2 = c ^ 2 ∧ a * b = 2 * n * (d ^ 2)

/-- Chain B [Rung 3/4 → BSD Congruent Numbers] (Heegner 1952 & Monsky 1990 Prime Slice):
    Every prime `p ≡ 5` or `7 (mod 8)` is a congruent number. Since no prime satisfies
    `p ≡ 6 (mod 8)`, this resolves the entire `ω(n) = 1` prime slice of the live Millennium
    bounty `millennium-bsd-congruent-squarefree`. -/
def statementProp : Prop :=
  ∀ (p : ℕ), Nat.Prime p → (p % 8 = 5 ∨ p % 8 = 7) → isCongruentNumber p
