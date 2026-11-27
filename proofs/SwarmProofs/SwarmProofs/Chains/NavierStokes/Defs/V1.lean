/-
Copyright 2026 Swarm Tips contributors
SPDX-License-Identifier: Apache-2.0
-/

import Mathlib.Data.Fin.Basic
import Mathlib.Data.Finset.Basic
import Mathlib.Data.Int.Basic
import Mathlib.Data.Real.Basic
import Mathlib.Data.Complex.Basic
import Mathlib.Analysis.Calculus.Deriv.Basic
import Mathlib.Analysis.SpecialFunctions.Pow.Real
import Mathlib.Algebra.BigOperators.Group.Finset.Basic

/-- Parameters for the Katz–Pavlović / Tao dissipative dyadic Navier–Stokes shell model:
    shell ratio `lam > 1`, nonlinear coupling exponent `alpha > 0` (`5/2` for 3D NS scaling),
    dissipation exponent `beta > 0` (`1` for standard Laplacian `-ν Δ`), and viscosity `nu ≥ 0`. -/
structure DyadicNSParams where
  lam : ℝ
  alpha : ℝ
  beta : ℝ
  nu : ℝ

/-- Admissible physical parameter regime for the dyadic Navier–Stokes shell cascade. -/
def DyadicNSParams.Valid (p : DyadicNSParams) : Prop :=
  1 < p.lam ∧ 0 < p.alpha ∧ 0 < p.beta ∧ 0 ≤ p.nu

/-- Right-hand side of shell `j ∈ Fin N` in the energy-conservative `N`-shell dyadic
    Navier–Stokes ODE:
    `du_j/dt = λ^{α j} u_{j-1}^2 - λ^{α (j+1)} u_j u_{j+1} - ν λ^{2 β j} u_j`. -/
noncomputable def dyadicRhs (N : ℕ) (p : DyadicNSParams) (u : Fin N → ℝ) (j : Fin N) : ℝ :=
  let incoming : ℝ :=
    if h : 0 < j.val then
      p.lam ^ (p.alpha * (j.val : ℝ)) * (u ⟨j.val - 1, Nat.lt_trans (Nat.sub_lt h Nat.zero_lt_one) j.isLt⟩) ^ 2
    else
      0
  let outgoing : ℝ :=
    if h : j.val + 1 < N then
      p.lam ^ (p.alpha * ((j.val + 1 : ℕ) : ℝ)) * u j * u ⟨j.val + 1, h⟩
    else
      0
  let viscous : ℝ := p.nu * p.lam ^ (2 * p.beta * (j.val : ℝ)) * u j
  incoming - outgoing - viscous

/-- Classical `C^1` solution of the `N`-shell dissipative dyadic Navier–Stokes ODE on `[0, T]`. -/
def IsDyadicNSSolution (N : ℕ) (p : DyadicNSParams) (T : ℝ) (u : ℝ → Fin N → ℝ) : Prop :=
  0 ≤ T ∧
  ∀ t ∈ Set.Icc (0 : ℝ) T, ∀ j : Fin N,
    HasDerivAt (fun s => u s j) (dyadicRhs N p (u t) j) t

/-- Total kinetic energy `E(u) = (1/2) ∑_{j=0}^{N-1} u_j^2` of an `N`-shell dyadic state. -/
noncomputable def dyadicEnergy (N : ℕ) (u : Fin N → ℝ) : ℝ :=
  (1 / 2 : ℝ) * Finset.sum Finset.univ (fun j : Fin N => (u j) ^ 2)

/-- Viscous dissipation functional `D(u) = ∑_{j=0}^{N-1} λ^{2 β j} u_j^2`. -/
noncomputable def dyadicDissipation (N : ℕ) (p : DyadicNSParams) (u : Fin N → ℝ) : ℝ :=
  Finset.sum Finset.univ (fun j : Fin N => p.lam ^ (2 * p.beta * (j.val : ℝ)) * (u j) ^ 2)

/-- Weighted Sobolev-type squared norm `‖u‖_{H^s}^2 = ∑_{j=0}^{N-1} λ^{2 s j} u_j^2`. -/
noncomputable def dyadicSobolevSq (N : ℕ) (p : DyadicNSParams) (s : ℝ) (u : Fin N → ℝ) : ℝ :=
  Finset.sum Finset.univ (fun j : Fin N => p.lam ^ (2 * s * (j.val : ℝ)) * (u j) ^ 2)

/-- Discrete integer 3D Fourier wavevector `k = (k_0, k_1, k_2) ∈ ℤ^3`. -/
abbrev Mode3 := Fin 3 → ℤ

/-- Finite cubic Fourier truncation box `[-M, M]^3 ⊂ ℤ^3` as a computable `Finset Mode3`. -/
def boxModes (M : ℕ) : Finset Mode3 :=
  Fintype.piFinset (fun _ : Fin 3 =>
    (Finset.range (2 * M + 1)).image (fun n : ℕ => (n : ℤ) - (M : ℤ)))

/-- Reality (Hermitian conjugation) symmetry `u(-k) = conj(u(k))` for a real-valued
    periodic vector field on `𝕋^3`. -/
def IsRealField3D (u : Mode3 → Fin 3 → ℂ) : Prop :=
  ∀ (k : Mode3) (c : Fin 3), u (fun j => -k j) c = star (u k c)

/-- Incompressibility (divergence-free constraint) `k · u(k) = 0` at every Fourier mode `k`. -/
def IsDivFree3D (u : Mode3 → Fin 3 → ℂ) : Prop :=
  ∀ k : Mode3,
    Finset.sum Finset.univ (fun c : Fin 3 => (k c : ℂ) * u k c) = 0

/-- Support inside the Galerkin truncation box `[-M, M]^3`. -/
def SupportedInBox (M : ℕ) (u : Mode3 → Fin 3 → ℂ) : Prop :=
  ∀ k : Mode3, k ∉ boxModes M → ∀ c : Fin 3, u k c = 0

/-- Complex inner product `⟨v, w⟩ = ∑_{c=0}^2 v_c · conj(w_c)` on `ℂ^3`. -/
def vecDotConj (v w : Fin 3 → ℂ) : ℂ :=
  Finset.sum Finset.univ (fun c : Fin 3 => v c * star (w c))

/-- Fourier advection coupling coefficient `i · (u_p · q)` for wavevectors `p, q ∈ ℤ^3`. -/
def advectScalar (up : Fin 3 → ℂ) (q : Mode3) : ℂ :=
  Complex.I * Finset.sum Finset.univ (fun c : Fin 3 => up c * (q c : ℂ))

/-- Exact 3D periodic Fourier–Galerkin trilinear advection form
    `b_M(u, v, w) = ∑_{p, q ∈ [-M,M]^3, p + q ∈ [-M,M]^3} i (u_p · q) ⟨v_q, w_{p+q}⟩`. -/
def nsTrilinear3D (M : ℕ) (u v w : Mode3 → Fin 3 → ℂ) : ℂ :=
  Finset.sum (boxModes M) fun p =>
    Finset.sum (boxModes M) fun q =>
      let pq : Mode3 := fun j => p j + q j
      if pq ∈ boxModes M then
        advectScalar (u p) q * vecDotConj (v q) (w pq)
      else
        0

/-- Squared Euclidean norm `|k|^2 = k_0^2 + k_1^2 + k_2^2` of a 3D Fourier wavevector. -/
def modeNormSq (k : Mode3) : ℕ :=
  Finset.sum Finset.univ (fun c : Fin 3 => (k c).natAbs ^ 2)

/-- Kinetic energy `E_M(u) = (1/2) ∑_{k ∈ [-M,M]^3} ∑_c ‖u_k,c‖^2` of a Galerkin field. -/
noncomputable def galerkinEnergy3D (M : ℕ) (u : Mode3 → Fin 3 → ℂ) : ℝ :=
  (1 / 2 : ℝ) * Finset.sum (boxModes M) fun k =>
    Finset.sum Finset.univ (fun c : Fin 3 => ‖u k c‖ ^ 2)

/-- Enstrophy `Ω_M(u) = (1/2) ∑_{k ∈ [-M,M]^3} |k|^2 ∑_c ‖u_k,c‖^2` of a Galerkin field. -/
noncomputable def galerkinEnstrophy3D (M : ℕ) (u : Mode3 → Fin 3 → ℂ) : ℝ :=
  (1 / 2 : ℝ) * Finset.sum (boxModes M) fun k =>
    (modeNormSq k : ℝ) * Finset.sum Finset.univ (fun c : Fin 3 => ‖u k c‖ ^ 2)

/-- Exact inviscid rational 3-shell conservation check helper for `#guard`. -/
def dyadicInviscidFluxSum3 (u0 u1 u2 c0 c1 : ℤ) : ℤ :=
  u0 * (-c0 * u0 * u1) + u1 * (c0 * u0 ^ 2 - c1 * u1 * u2) + u2 * (c1 * u1 ^ 2)

#guard (boxModes 0).card = 1
#guard (boxModes 1).card = 27
#guard modeNormSq (fun j => if j.val == 0 then 2 else -1) = 6
#guard dyadicInviscidFluxSum3 3 5 7 2 11 = 0
#guard dyadicInviscidFluxSum3 (-4) 9 (-2) 13 17 = 0
