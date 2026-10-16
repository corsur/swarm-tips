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

/-- The concrete N × N hollow dyadic Weil-Montgomery prime-harmonic Gram matrix at critical
    uncertainty scale T = N², whose off-diagonal (i, j) entry is the centered von Mangoldt
    (Λ(m) - 1) cosine correlation on the dyadic shell m ∈ [N + 1, 2N] at shift (i - j) · N. -/
noncomputable def weilPrimeGram (N : ℕ) : Matrix (Fin N) (Fin N) ℝ :=
  fun i j =>
    if i = j then 0
    else
      (1 / Real.log ((N : ℝ) + 2)) *
        ∑ m ∈ Finset.Icc (N + 1) (2 * N),
          ((weilVonMangoldt m - 1) / Real.sqrt (m : ℝ)) *
            Real.cos (((i.val : ℝ) - (j.val : ℝ)) * (N : ℝ) * Real.log (m : ℝ))

/-- Millennium Problem (Riemann Hypothesis — Weil-Gram Programme, Open Problem 1):
    Uniform cubic trace cancellation on the hollow dyadic Weil-Montgomery Gram family,
    normalized by the prime-harmonic sparsity scale √(log(N + 2) / N).
    Because `∃ C` is quantified OUTSIDE `∀ N ≥ 2`, `C` is a single universal constant
    independent of dimension `N` and cannot be chosen pointwise as `|tr(G^3)| / ...`. -/
def statementProp : Prop :=
  ∃ (C : ℝ), 0 ≤ C ∧
    ∀ (N : ℕ), 2 ≤ N →
      let G := weilPrimeGram N
      |Matrix.trace (G * G * G)| ≤
        C * Real.sqrt (Real.log ((N : ℝ) + 2) / (N : ℝ)) *
          (Matrix.trace (G * G) * Real.sqrt (Matrix.trace (G * G)))
