import Mathlib.Data.Nat.Prime.Basic

/-- A positive integer `n` is squarefree if no prime `p` has `p² ∣ n`. -/
def isSquarefreePos (n : ℕ) : Prop :=
  0 < n ∧ ∀ p ≤ n, Nat.Prime p → ¬ (p * p ∣ n)

/-- A positive integer `n` is a congruent number if it is the area (ab = 2nd²) of a
    right triangle with rational sides (a/d, b/d, c/d), which by classical 2-descent is
    strictly equivalent to rank(E_n(ℚ)) ≥ 1 on the elliptic curve E_n : y² = x³ - n²x. -/
def isCongruentNumber (n : ℕ) : Prop :=
  ∃ (a b c d : ℕ), 0 < a ∧ 0 < b ∧ 0 < c ∧ 0 < d ∧
    a ^ 2 + b ^ 2 = c ^ 2 ∧ a * b = 2 * n * (d ^ 2)

/-- Millennium Problem (Birch and Swinnerton-Dyer Conjecture — Congruent Number Rank-1 Case):
    For every squarefree positive integer n ≡ 5, 6, or 7 (mod 8), the functional equation
    root number of the Hasse–Weil L-function L(E_n, s) is w(E_n) = -1, forcing L(E_n, 1) = 0
    unconditionally. The Birch and Swinnerton-Dyer Conjecture predicts rank(E_n(ℚ)) ≥ 1,
    which is strictly equivalent to `isCongruentNumber n`. -/
def statementProp : Prop :=
  ∀ (n : ℕ), isSquarefreePos n → (n % 8 = 5 ∨ n % 8 = 6 ∨ n % 8 = 7) →
    isCongruentNumber n
