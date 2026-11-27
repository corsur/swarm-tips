-- @lean-policy: 4
-- @depends-on: chain-navier-stokes-2-katz-pavlovic-supercritical-blowup
import Mathlib.Data.Fin.Basic
import Mathlib.Data.Int.Basic
import Mathlib.Data.Complex.Basic
import SwarmProofs.Chains.NavierStokes.Defs.V1

/-- Chain P1 [Rung 3/4 → Navier–Stokes Regularity & Cascade] (3D Periodic Fourier–Galerkin Skew-Symmetry & 2D Enstrophy):
    Establishes the exact finite-dimensional Fourier–Galerkin conservation laws on the periodic
    3-torus `𝕋^3` across every cubic truncation `[-M, M]^3`:
    (1) Exact 3D skew-symmetry: for every real divergence-free `u` and real `v, w`,
        `b_M(u, v, w) + b_M(u, w, v) = 0` (both real and imaginary parts vanish identically;
        specializing `w = v` yields exact 3D advection energy orthogonality `b_M(u, v, v) = 0`);
    (2) Exact 2D planar enstrophy orthogonality: when `u` is additionally a 2D planar field
        (`u_k = 0` for `k_2 ≠ 0` and `u_{k,2} = 0`), `b_M(u, u, |k|^2 u) = 0` identically for EVERY
        truncation `M` (the exact cancellation eliminating vortex stretching in 2D Navier–Stokes).
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∀ (M : ℕ) (u v w : Mode3 → Fin 3 → ℂ),
    IsRealField3D u → IsDivFree3D u → IsRealField3D v → IsRealField3D w →
    ((nsTrilinear3D M u v w + nsTrilinear3D M u w v).re = 0 ∧
      (nsTrilinear3D M u v w + nsTrilinear3D M u w v).im = 0) ∧
    ((∀ k : Mode3, k ⟨2, by decide⟩ ≠ 0 → ∀ c : Fin 3, u k c = 0) →
      (∀ k : Mode3, u k ⟨2, by decide⟩ = 0) →
      (nsTrilinear3D M u u (fun k c => (modeNormSq k : ℂ) * u k c)).re = 0 ∧
      (nsTrilinear3D M u u (fun k c => (modeNormSq k : ℂ) * u k c)).im = 0)
