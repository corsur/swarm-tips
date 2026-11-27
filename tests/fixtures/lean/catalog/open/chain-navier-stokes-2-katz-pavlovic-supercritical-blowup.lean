-- @lean-policy: 4
-- @depends-on: chain-navier-stokes-1-dyadic-energy-subcritical
import Mathlib.Data.Fin.Basic
import Mathlib.Data.Real.Basic
import Mathlib.Analysis.SpecialFunctions.Pow.Real
import SwarmProofs.Chains.NavierStokes.Defs.V1

/-- Uniform-in-`N` finite-time `H^α` Sobolev norm blow-up witness for parameter regime `p`:
    fixed initial shell-0 amplitude `A > 0` and finite time `T > 0` such that for every ceiling `M`,
    all sufficiently large truncations `N ≥ N₀(M)` exceed `‖u(t)‖_{H^α}^2 > M` on `[0, T]`. -/
def HasSupercriticalDyadicBlowup (p : DyadicNSParams) : Prop :=
  ∃ (A T : ℝ), 0 < A ∧ 0 < T ∧
    ∀ (M : ℝ),
      ∃ (N₀ : ℕ), 1 ≤ N₀ ∧
        ∀ (N : ℕ), N₀ ≤ N →
          ∀ (hPos : 0 < N) (u : ℝ → Fin N → ℝ),
            IsDyadicNSSolution N p T u →
            u 0 ⟨0, hPos⟩ = A →
            (∀ j : Fin N, 0 < j.val → u 0 j = 0) →
            ∃ t ∈ Set.Icc (0 : ℝ) T, M < dyadicSobolevSq N p p.alpha (u t)

/-- Chain P1 [Rung 2/4 → Navier–Stokes Regularity & Cascade] (Katz–Pavlović 2005 / Cheskidov 2008 Supercritical Blow-Up):
    Unconditionally provable supercritical singularity mechanism: whenever the dissipation exponent
    lies strictly in Cheskidov's supercritical blow-up regime (`3 β < α`, including inviscid `ν = 0`
    Katz–Pavlović and weakly dissipative `ν > 0` Cheskidov cascades), there exist a single fixed
    low-mode initial amplitude `A > 0` and finite blow-up horizon `T > 0` (independent of `N`)
    such that for EVERY Sobolev ceiling `M : ℝ`, all sufficiently large finite truncations `N ≥ N₀(M)`
    launching from `u_0(0) = A, u_{j ≥ 1}(0) = 0` exceed `‖u(t)‖_{H^α}^2 > M` at some `t ∈ [0, T]`.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∀ (p : DyadicNSParams), p.Valid → 3 * p.beta < p.alpha → HasSupercriticalDyadicBlowup p
