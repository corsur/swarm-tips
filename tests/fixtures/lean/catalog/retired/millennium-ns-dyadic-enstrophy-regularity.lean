import Mathlib.Data.Int.Basic
import Mathlib.Data.Fintype.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic

open scoped BigOperators

/-- Dyadic kinetic energy E(u) = ∑_{n=0}^{N-1} u_n² across N shells. -/
def dyadicEnergy (N : ℕ) (u : Fin N → ℤ) : ℤ :=
  ∑ n : Fin N, (u n) ^ 2

/-- Dyadic viscous enstrophy dissipation D(u) = ∑_{n=0}^{N-1} 4^n u_n² (representing ∑ 2^{2n} u_n²). -/
def dyadicEnstrophy (N : ℕ) (u : Fin N → ℤ) : ℤ :=
  ∑ n : Fin N, ((4 : ℤ) ^ n.val) * (u n) ^ 2

/-- Supercritical 3D vortex-stretching trilinear production T(u) across N dyadic shells
    with alternating helical phase cancellation (-1)^n · 4^n · (n + 1) · u_n² · u_{n+1}. -/
def dyadicHelicalStretching (N : ℕ) (u : Fin (N + 1) → ℤ) : ℤ :=
  ∑ n : Fin N,
    ((-1 : ℤ) ^ n.val) * ((4 : ℤ) ^ n.val) * ((n.val : ℤ) + 1) *
      (u ⟨n.val, Nat.lt_succ_of_lt n.isLt⟩) ^ 2 *
      (u ⟨n.val + 1, Nat.succ_lt_succ n.isLt⟩)

/-- Millennium Problem (3D Navier–Stokes — Katz–Pavlović / Tao Dyadic Enstrophy Barrier):
    Uniform enstrophy-dissipation control of the helical vortex-stretching trilinear form
    across all shell cutoffs N ≥ 2 on monotone energy-bounded dyadic cascades. By degree-6
    scaling homogeneity (T(λu)² = λ⁶ T(u)² and E(λu)D(λu)² = λ⁶ E(u)D(u)²), the integer
    lattice formulation over `ℤ` is strictly equivalent to the rational/real formulation. -/
def statementProp : Prop :=
  ∃ (C : ℤ), 1 ≤ C ∧
    ∀ (N : ℕ), 2 ≤ N →
      ∀ (u : Fin (N + 1) → ℤ),
        (∀ n : Fin N, (u ⟨n.val + 1, Nat.succ_lt_succ n.isLt⟩) ^ 2 ≤ (u ⟨n.val, Nat.lt_succ_of_lt n.isLt⟩) ^ 2) →
          (dyadicHelicalStretching N u) ^ 2 ≤
            C * dyadicEnergy (N + 1) u * (dyadicEnstrophy (N + 1) u) ^ 2
