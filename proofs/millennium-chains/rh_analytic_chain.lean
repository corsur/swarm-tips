import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import Mathlib.Tactic.Ring
import Mathlib.Tactic.Linarith
import Mathlib.Tactic.Positivity

open scoped BigOperators

/-- Number of distinct prime factors ω(n) of `n`. -/
def primeFactorCount (n : ℕ) : ℕ :=
  ((Finset.range (n + 1)).filter (fun p => Nat.Prime p ∧ p ∣ n)).card

/-- Number of primes `p ≤ n` whose square `p²` divides `n`. -/
def squarePrimeFactorCount (n : ℕ) : ℕ :=
  ((Finset.range (n + 1)).filter (fun p => Nat.Prime p ∧ p * p ∣ n)).card

/-- The classical Möbius function μ(n) matching the Mertens hierarchy. -/
def mobiusInt (n : ℕ) : ℤ :=
  if 0 < n ∧ squarePrimeFactorCount n = 0 then (-1 : ℤ) ^ (primeFactorCount n) else 0

/-- The Mertens function M(N) = ∑_{n=0}^N μ(n) (with μ(0) = 0). -/
def mertensSum (N : ℕ) : ℤ :=
  ∑ n ∈ Finset.range (N + 1), mobiusInt n

/-- Live Mainnet Millennium Apex (`catalog:millennium-rh-mertens-quasi`, θ = 3/4):
    M(N)^4 ≤ C · N^3 for all N ≥ 1. -/
def MillenniumMertensQuasiRH : Prop :=
  ∃ (C : ℤ), 1 ≤ C ∧ ∀ (N : ℕ), 1 ≤ N → (mertensSum N) ^ 4 ≤ C * ((N : ℤ) ^ 3)

/-- Rung C4 (Strictly Weaker Open Strip Re(s) > 0.99, θ = 99/100):
    M(N)^100 ≤ C · N^99 for all N ≥ 1. -/
def RungC4 : Prop :=
  ∃ (C : ℤ), 1 ≤ C ∧ ∀ (N : ℕ), 1 ≤ N → (mertensSum N) ^ 100 ≤ C * ((N : ℤ) ^ 99)

/-- Rung C3 (Landau 1899 Prime Number Theorem Equivalent, M(N) = o(N)):
    ∀ k ≥ 1, ∃ N₀, ∀ N ≥ N₀, k · |M(N)| ≤ N. -/
def RungC3 : Prop :=
  ∀ (k : ℕ), 1 ≤ k → ∃ (N₀ : ℕ), ∀ (N : ℕ), N₀ ≤ N → (k : ℤ) * |mertensSum N| ≤ (N : ℤ)

/-- Master Implication Chain (Chain C):
    (1) `MillenniumMertensQuasiRH` (θ = 3/4) strictly implies `RungC4` (θ = 99/100);
    (2) `RungC4` (θ = 99/100) strictly implies `RungC3` (Prime Number Theorem M(N) = o(N)). -/
def statementProp : Prop :=
  (MillenniumMertensQuasiRH → RungC4) ∧ (RungC4 → RungC3)

theorem proof : statementProp := by
  constructor
  · rintro ⟨C, hC1, hQuasi⟩
    refine ⟨C ^ 25, one_le_pow₀ hC1, ?_⟩
    intro N hN1
    have h4 := hQuasi N hN1
    have hnonneg : (0 : ℤ) ≤ (mertensSum N) ^ 4 := by positivity
    have hpow25 : ((mertensSum N) ^ 4) ^ 25 ≤ (C * ((N : ℤ) ^ 3)) ^ 25 :=
      pow_le_pow_left₀ hnonneg h4 25
    have hN_int : (1 : ℤ) ≤ (N : ℤ) := by omega
    have hN_pow : (N : ℤ) ^ 75 ≤ (N : ℤ) ^ 99 :=
      pow_le_pow_right₀ hN_int (by decide)
    have hC25_nonneg : (0 : ℤ) ≤ C ^ 25 := by positivity
    calc
      (mertensSum N) ^ 100 = ((mertensSum N) ^ 4) ^ 25 := by ring
      _ ≤ (C * ((N : ℤ) ^ 3)) ^ 25 := hpow25
      _ = C ^ 25 * (N : ℤ) ^ 75 := by ring
      _ ≤ C ^ 25 * (N : ℤ) ^ 99 := mul_le_mul_of_nonneg_left hN_pow hC25_nonneg
  · rintro ⟨C, hC1, hStrip⟩ k hk1
    refine ⟨C.toNat * (k ^ 100) + 1, ?_⟩
    intro N hN0
    have hN1 : 1 ≤ N := by omega
    by_contra hlt
    have hlt' : (N : ℤ) < (k : ℤ) * |mertensSum N| := lt_of_not_ge hlt
    have hN_nonneg : (0 : ℤ) ≤ (N : ℤ) := by omega
    have hpow100 : (N : ℤ) ^ 100 < ((k : ℤ) * |mertensSum N|) ^ 100 :=
      pow_lt_pow_left₀ hlt' hN_nonneg (by decide)
    have habs100 : |mertensSum N| ^ 100 = (mertensSum N) ^ 100 := by
      have h1 : |mertensSum N| ^ 100 = (|mertensSum N| ^ 2) ^ 50 := by ring
      have h2 : (mertensSum N) ^ 100 = ((mertensSum N) ^ 2) ^ 50 := by ring
      rw [h1, h2, sq_abs]
    have hk_pow_nonneg : (0 : ℤ) ≤ (k : ℤ) ^ 100 := by positivity
    have hbound := hStrip N hN1
    have hupper : ((k : ℤ) * |mertensSum N|) ^ 100 ≤ (k : ℤ) ^ 100 * (C * (N : ℤ) ^ 99) := by
      calc
        ((k : ℤ) * |mertensSum N|) ^ 100 = (k : ℤ) ^ 100 * |mertensSum N| ^ 100 := by ring
        _ = (k : ℤ) ^ 100 * (mertensSum N) ^ 100 := by rw [habs100]
        _ ≤ (k : ℤ) ^ 100 * (C * (N : ℤ) ^ 99) := mul_le_mul_of_nonneg_left hbound hk_pow_nonneg
    have hC_cast : ((C.toNat : ℕ) : ℤ) = C := Int.toNat_of_nonneg (by omega)
    have hN_large : (k : ℤ) ^ 100 * C ≤ (N : ℤ) := by
      have hnat : C.toNat * (k ^ 100) ≤ N := by omega
      have hint : ((C.toNat * (k ^ 100) : ℕ) : ℤ) ≤ (N : ℤ) := Int.ofNat_le.mpr hnat
      push_cast at hint
      rw [hC_cast] at hint
      linarith
    have hN99_nonneg : (0 : ℤ) ≤ (N : ℤ) ^ 99 := by positivity
    have hcap : (k : ℤ) ^ 100 * (C * (N : ℤ) ^ 99) ≤ (N : ℤ) ^ 100 := by
      calc
        (k : ℤ) ^ 100 * (C * (N : ℤ) ^ 99) = ((k : ℤ) ^ 100 * C) * (N : ℤ) ^ 99 := by ring
        _ ≤ (N : ℤ) * (N : ℤ) ^ 99 := mul_le_mul_of_nonneg_right hN_large hN99_nonneg
        _ = (N : ℤ) ^ 100 := by ring
    exact lt_irrefl ((N : ℤ) ^ 100) (lt_of_lt_of_le hpow100 (le_trans hupper hcap))
