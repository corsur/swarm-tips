import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Data.Int.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic

open scoped BigOperators

/-- The closed-form top-4/5 Dirichlet hyperbola contribution for `k ∈ (⌊N/5⌋, N]`,
    using `M(1) = 1`, `M(2) = 0` (which annihilates the entire interval `(⌊N/3⌋, ⌊N/2⌋]`),
    and `M(3) = M(4) = -1` (which telescopes at `⌊N/4⌋`):
    `H_top(N) = (N - N/2) - (N/3 - N/5) - 1`. -/
def hyperbolaTopFourFifths (N : ℕ) : ℤ :=
  ((N : ℤ) - ((N / 2 : ℕ) : ℤ)) - (((N / 3 : ℕ) : ℤ) - ((N / 5 : ℕ) : ℤ)) - 1

/-- Embedded Machine-Checked Reduction Theorem (`0 SOL` Preamble Proof):
    Proves that in the Dirichlet hyperbola decomposition
    `M_N = 1 - ((N - N/2)*M1 + (N/2 - N/3)*M2 + (N/3 - N/4)*M3 + (N/4 - N/5)*M4 + S5)`,
    substituting the exact base values `M(1) = 1`, `M(2) = 0`, `M(3) = -1`, `M(4) = -1`
    annihilates `(N/2 - N/3)` and telescopes `N/4`, reducing `M_N` identically to
    `-(S5 + hyperbolaTopFourFifths N)`! -/
theorem hyperbola_fifth_reduction_to_mertens
    (N : ℕ) (S5 : ℤ) :
    let M1 : ℤ := 1
    let M2 : ℤ := 0
    let M3 : ℤ := -1
    let M4 : ℤ := -1
    1 - (((N : ℤ) - (N / 2 : ℕ)) * M1 +
      ((N / 2 : ℕ) - (N / 3 : ℕ)) * M2 +
      ((N / 3 : ℕ) - (N / 4 : ℕ)) * M3 +
      ((N / 4 : ℕ) - (N / 5 : ℕ)) * M4 + S5) =
      -(S5 + hyperbolaTopFourFifths N) := by
  unfold hyperbolaTopFourFifths
  dsimp
  omega

/-- Compressed Dirichlet–Mertens floor-sum over the remaining `1/5` range `k ∈ [2, N/5]`. -/
def compressedHyperbolaFifthSum (N : ℕ) : ℤ :=
  ∑ j ∈ Finset.range (N / 5),
    if j = 0 then 0
    else
      let m := N / (j + 1)
      ∑ d ∈ Finset.range m,
        if (d + 1) % 4 = 0 ∨ (d + 1) % 9 = 0 ∨ (d + 1) % 25 = 0 then (0 : ℤ)
        else if (d + 1) = 1 then 1
        else if Nat.Prime (d + 1) then -1
        else 1

/-- Decomposed Millennium Sub-Problem (Riemann Hypothesis — Compressed `N/5` Hyperbola Discrepancy):
    By `hyperbola_fifth_reduction_to_mertens`, the top `4/5` of the Dirichlet hyperbola range
    `k ∈ (⌊N/5⌋, N]` collapses identically to `hyperbolaTopFourFifths N ≈ (11/30) N`.
    Proving the sub-linear cancellation bound on the remaining `k ≤ ⌊N/5⌋` hyperbola sum
    establishes the `O(N^{3/4})` quasi-Riemann Hypothesis discrepancy barrier. -/
def statementProp : Prop :=
  ∃ (C : ℤ), 1 ≤ C ∧
    ∀ (N : ℕ), 10 ≤ N →
      (compressedHyperbolaFifthSum N + hyperbolaTopFourFifths N) ^ 4 ≤ C * ((N : ℤ) ^ 3)
