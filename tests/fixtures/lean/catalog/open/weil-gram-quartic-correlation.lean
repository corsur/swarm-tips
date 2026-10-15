import Mathlib.LinearAlgebra.Matrix.Trace
import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import Mathlib.Analysis.SpecialFunctions.Trigonometric.Basic

open scoped BigOperators Matrix
open Classical

/-- The von Mangoldt function Λ(m): equals `log p` if `m = p^k` is a prime power
    (characterized by `1 < m` and every prime divisor of `m` equaling `m.minFac`),
    and `0` otherwise. -/
noncomputable def weilVonMangoldt (m : ℕ) : ℝ :=
  if 1 < m ∧ ∀ p ≤ m, Nat.Prime p → p ∣ m → p = Nat.minFac m then
    Real.log (Nat.minFac m : ℝ)
  else 0

/-- The concrete N × N Weil explicit-formula prime-logarithm Gram matrix at critical
    zero-spacing bandwidth λ = 1, whose (i, j) entry is the prime-power von Mangoldt
    Fourier-cosine correlation at shift (i - j) / log(N + 2). -/
noncomputable def weilPrimeGram (N : ℕ) : Matrix (Fin N) (Fin N) ℝ :=
  fun i j =>
    (1 / Real.log ((N : ℝ) + 2)) *
      ∑ m ∈ Finset.Icc 1 N,
        (weilVonMangoldt m / Real.sqrt (m : ℝ)) *
          Real.cos (((i.val : ℝ) - (j.val : ℝ)) * Real.log (m : ℝ) / Real.log ((N : ℝ) + 2))

/-- Millennium Problem (Riemann Hypothesis — Weil-Gram Programme, Open Problem 2):
    Uniform quartic cumulant / 4-point correlation bound on the concrete Weil
    prime-logarithm Gram family (Montgomery-Odlyzko GUE 4-point cancellation).
    The universal constant `C` is quantified OUTSIDE `∀ N ≥ 2`. -/
def statementProp : Prop :=
  ∃ (C : ℝ), 0 ≤ C ∧
    ∀ (N : ℕ), 2 ≤ N →
      let G := weilPrimeGram N
      |Matrix.trace (G * G * G * G) - (Matrix.trace (G * G)) ^ 2 / (N : ℝ)| ≤
        C * (1 / (N : ℝ)) * (Matrix.trace (G * G)) ^ 2
