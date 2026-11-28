-- @lean-policy: 4
import Mathlib.Data.Fin.Basic
import Mathlib.Data.Int.Basic
import Mathlib.Data.Real.Basic
import Mathlib.Data.Complex.Basic
import Mathlib.Analysis.Complex.Basic
import Mathlib.Analysis.Calculus.Deriv.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import SwarmProofs.Chains.NavierStokes.Defs.V1

/-- Leray orthogonal projection `ℙ_k w = w - ((k · w) / |k|^2) k` onto divergence-free modes
    at wavevector `k ∈ ℤ^3` (identity at `k = 0`). -/
noncomputable def lerayProj3D (k : Mode3) (w : Fin 3 → ℂ) : Fin 3 → ℂ :=
  if modeNormSq k = 0 then
    w
  else
    let dotKW : ℂ := Finset.sum Finset.univ (fun j : Fin 3 => (k j : ℂ) * w j)
    fun c => w c - (dotKW / (modeNormSq k : ℂ)) * (k c : ℂ)

/-- Raw truncated Fourier–Galerkin advection convolution `B_M(u, u)_k = ∑_{p, k-p ∈ [-M,M]^3} i(u_p · (k-p)) u_{k-p}`. -/
def galerkinAdvect3D (M : ℕ) (u : Mode3 → Fin 3 → ℂ) (k : Mode3) (c : Fin 3) : ℂ :=
  Finset.sum (boxModes M) fun p =>
    let q : Mode3 := fun j => k j - p j
    if q ∈ boxModes M then
      advectScalar (u p) q * u q c
    else
      0

/-- Right-hand side of the 3D periodic incompressible Fourier–Galerkin Navier–Stokes ODE at mode `k`:
    `du_k/dt = -ℙ_k B_M(u, u)_k - ν |k|^2 u_k`. -/
noncomputable def galerkinNS3DRhs (M : ℕ) (ν : ℝ) (u : Mode3 → Fin 3 → ℂ) (k : Mode3) (c : Fin 3) : ℂ :=
  - lerayProj3D k (galerkinAdvect3D M u k) c - (ν : ℂ) * (modeNormSq k : ℂ) * u k c

/-- Classical `C^1` solution of the 3D periodic Fourier–Galerkin Navier–Stokes system on `[0, T]`
    at cubic truncation `[-M, M]^3`. -/
def IsGalerkin3DNSSolution (M : ℕ) (ν T : ℝ) (u : ℝ → Mode3 → Fin 3 → ℂ) : Prop :=
  0 ≤ T ∧
  (∀ t ∈ Set.Icc (0 : ℝ) T, IsRealField3D (u t) ∧ IsDivFree3D (u t) ∧ SupportedInBox M (u t)) ∧
  (∀ t ∈ Set.Icc (0 : ℝ) T, ∀ k ∈ boxModes M, ∀ c : Fin 3,
    HasDerivAt (fun s => u s k c) (galerkinNS3DRhs M ν (u t) k c) t)

/-- Existence of a truncation-independent enstrophy ceiling `K` on `[0, T]` across all
    3D periodic Galerkin truncations `M ≥ M₀` launching from initial datum `u₀`. -/
def HasUniformGalerkin3DEnstrophyBound
    (ν : ℝ) (M₀ : ℕ) (u₀ : Mode3 → Fin 3 → ℂ) (T : ℝ) : Prop :=
  ∃ (K : ℝ), 0 ≤ K ∧
    ∀ (M : ℕ), M₀ ≤ M →
      ∀ (u : ℝ → Mode3 → Fin 3 → ℂ),
        IsGalerkin3DNSSolution M ν T u →
        u 0 = u₀ →
        ∀ t ∈ Set.Icc (0 : ℝ) T, galerkinEnstrophy3D M (u t) ≤ K

/-- Chain P1 [Rung 4/4 → Millennium Frontier: 3D Periodic Navier–Stokes Uniform Enstrophy Regularity]:
    Direct finite-Galerkin formulation of the Clay Millennium Prize Problem for 3D periodic
    incompressible Navier–Stokes: for every viscosity `ν > 0`, every smooth trigonometric-polynomial
    divergence-free initial datum `u₀` supported in `[-M₀, M₀]^3`, and every time horizon `T ≥ 0`,
    there exists a uniform enstrophy ceiling `K` INDEPENDENT OF the Galerkin truncation `M ≥ M₀`
    such that every 3D Galerkin solution `u` on `[0, T]` with `u(0) = u₀` satisfies
    `Ω_M(u(t)) ≤ K` for all `t ∈ [0, T]` (equivalent by Foias–Temam / BKM to global `C^∞` regularity).
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∀ (ν : ℝ), 0 < ν →
    ∀ (M₀ : ℕ) (u₀ : Mode3 → Fin 3 → ℂ),
      IsRealField3D u₀ → IsDivFree3D u₀ → SupportedInBox M₀ u₀ →
      ∀ (T : ℝ), 0 ≤ T →
        HasUniformGalerkin3DEnstrophyBound ν M₀ u₀ T
