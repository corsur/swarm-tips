/-
Copyright 2026 Swarm Tips contributors
SPDX-License-Identifier: Apache-2.0
-/

import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import Mathlib.NumberTheory.Primorial
import Mathlib.NumberTheory.ArithmeticFunction.Defs
import Mathlib.NumberTheory.ArithmeticFunction.Moebius
import Mathlib.NumberTheory.ArithmeticFunction.Misc

/-- Number of distinct prime factors ω(n) of `n`. -/
def primeFactorCount (n : ℕ) : ℕ :=
  ((Finset.range (n + 1)).filter (fun p => Nat.Prime p ∧ p ∣ n)).card

#guard primeFactorCount 1 = 0
#guard primeFactorCount 6 = 2
#guard primeFactorCount 12 = 2
#guard primeFactorCount 30 = 3

/-- Number of primes `p ≤ n` whose square `p²` divides `n`. -/
def squarePrimeFactorCount (n : ℕ) : ℕ :=
  ((Finset.range (n + 1)).filter (fun p => Nat.Prime p ∧ p * p ∣ n)).card

#guard squarePrimeFactorCount 1 = 0
#guard squarePrimeFactorCount 4 = 1
#guard squarePrimeFactorCount 6 = 0
#guard squarePrimeFactorCount 12 = 1

/-- The classical Möbius function μ(n) matching the Mertens hierarchy. -/
def mobiusInt (n : ℕ) : ℤ :=
  if 0 < n ∧ squarePrimeFactorCount n = 0 then (-1 : ℤ) ^ (primeFactorCount n) else 0

#guard mobiusInt 1 = 1
#guard mobiusInt 2 = -1
#guard mobiusInt 3 = -1
#guard mobiusInt 4 = 0
#guard mobiusInt 5 = -1
#guard mobiusInt 6 = 1

-- Meissel's Möbius floor-sum identity sanity checks for N = 1..10:
#guard Finset.sum (Finset.range 1) (fun k => mobiusInt (k + 1) * ((1 / (k + 1) : ℕ) : ℤ)) = 1
#guard Finset.sum (Finset.range 2) (fun k => mobiusInt (k + 1) * ((2 / (k + 1) : ℕ) : ℤ)) = 1
#guard Finset.sum (Finset.range 3) (fun k => mobiusInt (k + 1) * ((3 / (k + 1) : ℕ) : ℤ)) = 1
#guard Finset.sum (Finset.range 4) (fun k => mobiusInt (k + 1) * ((4 / (k + 1) : ℕ) : ℤ)) = 1
#guard Finset.sum (Finset.range 5) (fun k => mobiusInt (k + 1) * ((5 / (k + 1) : ℕ) : ℤ)) = 1
#guard Finset.sum (Finset.range 6) (fun k => mobiusInt (k + 1) * ((6 / (k + 1) : ℕ) : ℤ)) = 1
#guard Finset.sum (Finset.range 7) (fun k => mobiusInt (k + 1) * ((7 / (k + 1) : ℕ) : ℤ)) = 1
#guard Finset.sum (Finset.range 8) (fun k => mobiusInt (k + 1) * ((8 / (k + 1) : ℕ) : ℤ)) = 1
#guard Finset.sum (Finset.range 9) (fun k => mobiusInt (k + 1) * ((9 / (k + 1) : ℕ) : ℤ)) = 1
#guard Finset.sum (Finset.range 10) (fun k => mobiusInt (k + 1) * ((10 / (k + 1) : ℕ) : ℤ)) = 1

/-- The Mertens function M(N) = ∑_{n=0}^N μ(n) (with μ(0) = 0). -/
def mertensSum (N : ℕ) : ℤ :=
  Finset.sum (Finset.range (N + 1)) (fun n => mobiusInt n)

#guard mertensSum 1 = 1
#guard mertensSum 2 = 0
#guard mertensSum 3 = -1
#guard mertensSum 6 = -1
#guard mertensSum 10 = -1

/-- Canonical bridge lemma matching `primeFactorCount` with `ArithmeticFunction.cardDistinctFactors`. -/
theorem bridge_primeFactorCount (n : ℕ) :
    primeFactorCount n = ArithmeticFunction.cardDistinctFactors n := by
  rcases eq_or_ne n 0 with rfl | hn
  · simp [primeFactorCount, Nat.not_prime_zero]
  · unfold primeFactorCount
    have hset : (Finset.range (n + 1)).filter (fun p => Nat.Prime p ∧ p ∣ n) = n.primeFactors := by
      ext p
      simp only [Finset.mem_filter, Finset.mem_range, Nat.mem_primeFactors]
      exact ⟨fun h => ⟨h.2.1, h.2.2, hn⟩,
        fun h => ⟨Nat.lt_succ_of_le (Nat.le_of_dvd (Nat.pos_of_ne_zero hn) h.2.1), h.1, h.2.1⟩⟩
    rw [hset, ArithmeticFunction.cardDistinctFactors_apply, Nat.primeFactors, ← List.card_toFinset]

/-- Canonical bridge lemma matching `mobiusInt` with `ArithmeticFunction.moebius`. -/
theorem bridge_mobiusInt (n : ℕ) :
    mobiusInt n = ArithmeticFunction.moebius n := by
  rcases eq_or_ne n 0 with rfl | hn
  · simp [mobiusInt]
  · have hpos : 0 < n := Nat.pos_of_ne_zero hn
    have hsq_iff : (0 < n ∧ squarePrimeFactorCount n = 0) ↔ Squarefree n := by
      rw [and_iff_right hpos, squarePrimeFactorCount, Finset.card_eq_zero, Finset.filter_eq_empty_iff,
        Nat.squarefree_iff_prime_squarefree]
      constructor
      · intro hemp p hp hpp
        have hle : p ≤ n := Nat.le_of_dvd hpos (dvd_trans ⟨p, rfl⟩ hpp)
        exact hemp (Finset.mem_range.mpr (Nat.lt_succ_of_le hle)) ⟨hp, hpp⟩
      · intro hsq p _ hp
        exact hsq p hp.1 hp.2
    unfold mobiusInt
    by_cases hsq : Squarefree n
    · rw [if_pos (hsq_iff.mpr hsq), ArithmeticFunction.moebius_apply_of_squarefree hsq,
        bridge_primeFactorCount,
        (ArithmeticFunction.cardDistinctFactors_eq_cardFactors_iff_squarefree hn).mpr hsq]
    · rw [if_neg (fun h => hsq (hsq_iff.mp h)), ArithmeticFunction.moebius]
      simp [hsq]

/-- Canonical bridge lemma matching the prime product with Mathlib's `primorial N` (never `@[simp]`). -/
theorem bridge_primorial (N : ℕ) :
    Finset.prod ((Finset.range (N + 1)).filter Nat.Prime) (fun p : ℕ => p) = primorial N :=
  rfl
