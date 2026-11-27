import Mathlib.Data.Fin.Basic
import Mathlib.Data.Finset.Basic
import Mathlib.Data.Real.Basic
import Mathlib.Analysis.SpecialFunctions.Pow.Real
import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import Mathlib.Tactic.Linarith
import Mathlib.Tactic.NormNum
import Mathlib.Tactic.Ring

/-- Parameters for the Katz–Pavlović / Tao dissipative dyadic Navier–Stokes shell model. -/
structure DyadicNSParams where
  lam : ℝ
  alpha : ℝ
  beta : ℝ
  nu : ℝ

def DyadicNSParams.Valid (p : DyadicNSParams) : Prop :=
  1 < p.lam ∧ 0 < p.alpha ∧ 0 < p.beta ∧ 0 ≤ p.nu

noncomputable def dyadicRhs (N : ℕ) (p : DyadicNSParams) (u : Fin N → ℝ) (j : Fin N) : ℝ :=
  let incoming : ℝ :=
    if h : 0 < j.val then
      p.lam ^ (p.alpha * (j.val : ℝ)) *
        (u ⟨j.val - 1, Nat.lt_trans (Nat.sub_lt h Nat.zero_lt_one) j.isLt⟩) ^ 2
    else
      0
  let outgoing : ℝ :=
    if h : j.val + 1 < N then
      p.lam ^ (p.alpha * ((j.val + 1 : ℕ) : ℝ)) * u j * u ⟨j.val + 1, h⟩
    else
      0
  let viscous : ℝ := p.nu * p.lam ^ (2 * p.beta * (j.val : ℝ)) * u j
  incoming - outgoing - viscous

noncomputable def dyadicDissipation (N : ℕ) (p : DyadicNSParams) (u : Fin N → ℝ) : ℝ :=
  Finset.sum Finset.univ (fun j : Fin N => p.lam ^ (2 * p.beta * (j.val : ℝ)) * (u j) ^ 2)

/-- Master Synthesis & Scaling Dichotomy Theorem (Chain P1 — Navier–Stokes Regularity):
    (1) Exact arbitrary-`N` algebraic telescoping identity of the dyadic shell cascade:
        `∑_{j=0}^{N-1} u_j · dyadicRhs N p u j = -ν · dyadicDissipation N p u` for EVERY `N`, `p`, `u`;
    (2) Exact disjointness of the subcritical global regularity regime (`α < 2 β`, Rung P1-1)
        and Cheskidov's supercritical blow-up regime (`3 β < α`, Rung P1-2);
    (3) 3D Navier–Stokes scaling witness (`α = 5/2, β = 1`) lies strictly inside the critical
        intermediate gap between the two provable regimes (`¬(5/2 < 2·1) ∧ ¬(3·1 < 5/2)`). -/
def statementProp : Prop :=
  (∀ (N : ℕ) (p : DyadicNSParams) (u : Fin N → ℝ),
    Finset.sum Finset.univ (fun j : Fin N => u j * dyadicRhs N p u j) =
      - p.nu * dyadicDissipation N p u) ∧
  (∀ (p : DyadicNSParams), p.Valid → ¬ (p.alpha < 2 * p.beta ∧ 3 * p.beta < p.alpha)) ∧
  (let p3d : DyadicNSParams := { lam := 2, alpha := 5 / 2, beta := 1, nu := 1 };
    p3d.Valid ∧ ¬ (p3d.alpha < 2 * p3d.beta) ∧ ¬ (3 * p3d.beta < p3d.alpha))

lemma flux_telescope (N : ℕ) (p : DyadicNSParams) (u : Fin N → ℝ) :
    Finset.sum Finset.univ (fun j : Fin N =>
      u j * (if h : 0 < j.val then
        p.lam ^ (p.alpha * (j.val : ℝ)) *
          (u ⟨j.val - 1, Nat.lt_trans (Nat.sub_lt h Nat.zero_lt_one) j.isLt⟩) ^ 2
      else 0)) =
    Finset.sum Finset.univ (fun j : Fin N =>
      u j * (if h : j.val + 1 < N then
        p.lam ^ (p.alpha * ((j.val + 1 : ℕ) : ℝ)) * u j * u ⟨j.val + 1, h⟩
      else 0)) := by
  cases N with
  | zero => simp
  | succ m =>
    have hL :
        Finset.sum Finset.univ (fun j : Fin (m + 1) =>
          u j * (if h : 0 < j.val then
            p.lam ^ (p.alpha * (j.val : ℝ)) *
              (u ⟨j.val - 1, Nat.lt_trans (Nat.sub_lt h Nat.zero_lt_one) j.isLt⟩) ^ 2
          else 0)) =
        Finset.sum Finset.univ (fun k : Fin m =>
          u k.succ * (p.lam ^ (p.alpha * ((k.val + 1 : ℕ) : ℝ)) * (u k.castSucc) ^ 2)) := by
      rw [Fin.sum_univ_succ]
      simp only [Fin.val_zero, lt_self_iff_false, ↓reduceDIte, mul_zero, zero_add]
      refine Finset.sum_congr rfl ?_
      intro k _
      have hpos : 0 < k.succ.val := Nat.succ_pos k.val
      rw [dif_pos hpos]
      simp only [Fin.val_succ, Nat.add_sub_cancel]
      rfl
    have hR :
        Finset.sum Finset.univ (fun j : Fin (m + 1) =>
          u j * (if h : j.val + 1 < m + 1 then
            p.lam ^ (p.alpha * ((j.val + 1 : ℕ) : ℝ)) * u j * u ⟨j.val + 1, h⟩
          else 0)) =
        Finset.sum Finset.univ (fun k : Fin m =>
          u k.castSucc * (p.lam ^ (p.alpha * ((k.val + 1 : ℕ) : ℝ)) * u k.castSucc * u k.succ)) := by
      rw [Fin.sum_univ_castSucc]
      simp only [Fin.val_last, lt_self_iff_false, ↓reduceDIte, mul_zero, add_zero]
      refine Finset.sum_congr rfl ?_
      intro k _
      have hlt : k.castSucc.val + 1 < m + 1 := Nat.succ_lt_succ k.isLt
      rw [dif_pos hlt]
      simp only [Fin.val_castSucc]
      rfl
    rw [hL, hR]
    refine Finset.sum_congr rfl ?_
    intro k _
    ring

theorem proof : statementProp := by
  refine ⟨?_, ?_, ?_⟩
  · intro N p u
    have hsplit :
        Finset.sum Finset.univ (fun j : Fin N => u j * dyadicRhs N p u j) =
        (Finset.sum Finset.univ (fun j : Fin N =>
          u j * (if h : 0 < j.val then
            p.lam ^ (p.alpha * (j.val : ℝ)) *
              (u ⟨j.val - 1, Nat.lt_trans (Nat.sub_lt h Nat.zero_lt_one) j.isLt⟩) ^ 2
          else 0)) -
         Finset.sum Finset.univ (fun j : Fin N =>
          u j * (if h : j.val + 1 < N then
            p.lam ^ (p.alpha * ((j.val + 1 : ℕ) : ℝ)) * u j * u ⟨j.val + 1, h⟩
          else 0))) -
        Finset.sum Finset.univ (fun j : Fin N =>
          u j * (p.nu * p.lam ^ (2 * p.beta * (j.val : ℝ)) * u j)) := by
      rw [← Finset.sum_sub_distrib, ← Finset.sum_sub_distrib]
      refine Finset.sum_congr rfl ?_
      intro j _
      unfold dyadicRhs
      ring
    rw [hsplit, flux_telescope N p u, sub_self, zero_sub]
    unfold dyadicDissipation
    rw [neg_mul, Finset.mul_sum]
    congr 1
    refine Finset.sum_congr rfl ?_
    intro j _
    ring
  · intro p hp ⟨h1, h2⟩
    have hβ : 0 < p.beta := hp.2.2.1
    linarith
  · dsimp only
    refine ⟨⟨by norm_num, by norm_num, by norm_num, by norm_num⟩, by norm_num, by norm_num⟩
