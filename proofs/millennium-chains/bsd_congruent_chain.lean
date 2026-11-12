import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Data.Rat.Defs
import Mathlib.Algebra.Field.Defs

/-- A positive integer `n` is squarefree if no prime `p ≤ n` has `p² ∣ n`. -/
def isSquarefreePos (n : ℕ) : Prop :=
  0 < n ∧ ∀ p ≤ n, Nat.Prime p → ¬ (p * p ∣ n)

/-- A positive integer `n` is a congruent number if `ab = 2nd²` and `a² + b² = c²`
    for positive integers `a, b, c, d`. -/
def isCongruentNumber (n : ℕ) : Prop :=
  ∃ (a b c d : ℕ), 0 < a ∧ 0 < b ∧ 0 < c ∧ 0 < d ∧
    a ^ 2 + b ^ 2 = c ^ 2 ∧ a * b = 2 * n * (d ^ 2)

/-- Rung B1: Koblitz 2-descent bijection between congruent numbers and non-2-torsion
    rational points on E_n : y² = x³ - n²x. -/
def RungB1 : Prop :=
  ∀ (n : ℕ), 1 ≤ n →
    (isCongruentNumber n ↔ ∃ (x y : ℚ), y ≠ 0 ∧ y ^ 2 = x ^ 3 - (n : ℚ) ^ 2 * x)

/-- Rung B2: Fermat's right-triangle theorem (n = 1 is not congruent; Mordell–Weil rank 0). -/
def RungB2 : Prop :=
  ¬ isCongruentNumber 1

/-- Rung B3: Heegner (1952) & Monsky (1990) theorem — every prime p ≡ 5, 7 (mod 8) is congruent. -/
def RungB3 : Prop :=
  ∀ (p : ℕ), Nat.Prime p → (p % 8 = 5 ∨ p % 8 = 7) → isCongruentNumber p

/-- Rung B4: Open semiprime BSD slice (ω(n) = 2) — every product of two distinct primes
    pq ≡ 5, 6, 7 (mod 8) is congruent. -/
def RungB4 : Prop :=
  ∀ (p q : ℕ), Nat.Prime p → Nat.Prime q → p < q →
    ((p * q) % 8 = 5 ∨ (p * q) % 8 = 6 ∨ (p * q) % 8 = 7) →
      isCongruentNumber (p * q)

/-- Remaining ω(n) ≥ 3 squarefree slice (squarefree n ≡ 5, 6, 7 mod 8 that is neither
    prime nor a product of two distinct primes). -/
def HigherSquarefreeSlice : Prop :=
  ∀ (n : ℕ), isSquarefreePos n →
    (n % 8 = 5 ∨ n % 8 = 6 ∨ n % 8 = 7) →
    ¬ Nat.Prime n →
    (¬ ∃ p q : ℕ, Nat.Prime p ∧ Nat.Prime q ∧ p < q ∧ n = p * q) →
      isCongruentNumber n

/-- Live Mainnet Millennium Apex (`catalog:millennium-bsd-congruent-squarefree`):
    Every squarefree integer n ≡ 5, 6, 7 (mod 8) is congruent. -/
def MillenniumBSDCongruent : Prop :=
  ∀ (n : ℕ), isSquarefreePos n → (n % 8 = 5 ∨ n % 8 = 6 ∨ n % 8 = 7) →
    isCongruentNumber n

/-- Master Reduction Theorem (Chain B):
    (1) Rung B1 + Rung B2 immediately prove E_1(ℚ) has no non-2-torsion affine points y ≠ 0;
    (2) Rung B3 (ω = 1 primes, automatically excluding 6 mod 8) + Rung B4 (ω = 2 semiprimes) +
        the ω ≥ 3 slice partition and imply `MillenniumBSDCongruent`. -/
def statementProp : Prop :=
  (RungB1 → RungB2 → ¬ ∃ (x y : ℚ), y ≠ 0 ∧ y ^ 2 = x ^ 3 - x) ∧
    (RungB3 → RungB4 → HigherSquarefreeSlice → MillenniumBSDCongruent)

theorem proof : statementProp := by
  constructor
  · intro hB1 hB2 ⟨x, y, hy, heq⟩
    have hiff := hB1 1 (by decide)
    apply hB2
    apply hiff.mpr
    refine ⟨x, y, hy, ?_⟩
    simpa using heq
  · intro hB3 hB4 hHigh n hSq hMod
    by_cases hPrime : Nat.Prime n
    · rcases hMod with h5 | h6 | h7
      · exact hB3 n hPrime (Or.inl h5)
      · exfalso
        rcases hPrime.eq_two_or_odd with rfl | hOdd
        · omega
        · omega
      · exact hB3 n hPrime (Or.inr h7)
    · by_cases hSemi : ∃ p q : ℕ, Nat.Prime p ∧ Nat.Prime q ∧ p < q ∧ n = p * q
      · obtain ⟨p, q, hp, hq, hpq, rfl⟩ := hSemi
        exact hB4 p q hp hq hpq hMod
      · exact hHigh n hSq hMod hPrime hSemi
