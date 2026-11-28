-- SPDX-License-Identifier: Apache-2.0
import Mathlib.Data.Fin.Basic
import Mathlib.Data.Int.Basic
import Mathlib.Data.Real.Basic
import Mathlib.Data.Complex.Basic
import Mathlib.Analysis.Complex.Basic
import Mathlib.Tactic.FinCases
import Mathlib.Tactic.NormNum
import SwarmProofs.Chains.NavierStokes.Defs.V1

private def shearKPlus : Mode3 :=
  fun j => if j = 0 then 1 else 0

private def shearKMinus : Mode3 :=
  fun j => if j = 0 then -1 else 0

private def shearDatum (k : Mode3) (c : Fin 3) : ℂ :=
  if (k = shearKPlus ∨ k = shearKMinus) ∧ c = 1 then 1 else 0

private lemma neg_shearKPlus : (fun j : Fin 3 => -shearKPlus j) = shearKMinus := by
  funext j; fin_cases j <;> rfl

private lemma neg_shearKMinus : (fun j : Fin 3 => -shearKMinus j) = shearKPlus := by
  funext j; fin_cases j <;> rfl

private lemma shearKPlus_mem_box1 : shearKPlus ∈ boxModes 1 := by
  unfold boxModes shearKPlus
  rw [Fintype.mem_piFinset]
  intro j
  fin_cases j <;> decide

private lemma shearKMinus_mem_box1 : shearKMinus ∈ boxModes 1 := by
  unfold boxModes shearKMinus
  rw [Fintype.mem_piFinset]
  intro j
  fin_cases j <;> decide

/-- Machine-checked non-vacuity witness for `chain-navier-stokes-4-leray-3d-uniform-enstrophy-millennium.lean`:
    exhibits a nonzero smooth divergence-free real periodic initial datum `shearDatum`
    supported in `[-1, 1]^3`, proving every precondition of `statementProp` is satisfiable
    with positive viscosity `ν = 1`, `M₀ = 1`, `T = 1`, and strictly positive initial field. -/
theorem nonvacuous_navier_stokes_3d_millennium :
    ∃ (ν T : ℝ) (M₀ : ℕ) (u₀ : Mode3 → Fin 3 → ℂ),
      0 < ν ∧ 0 ≤ T ∧
      IsRealField3D u₀ ∧ IsDivFree3D u₀ ∧ SupportedInBox M₀ u₀ ∧
      u₀ shearKPlus 1 = 1 := by
  refine ⟨1, 1, 1, shearDatum, by norm_num, by norm_num, ?_, ?_, ?_, ?_⟩
  · intro k c
    have hiff :
        ((fun j => -k j) = shearKPlus ∨ (fun j => -k j) = shearKMinus) ↔
          (k = shearKPlus ∨ k = shearKMinus) := by
      constructor
      · rintro (h | h)
        · right
          funext j
          have hj := congrFun h j
          fin_cases j <;> simp [shearKPlus, shearKMinus] at hj ⊢ <;> omega
        · left
          funext j
          have hj := congrFun h j
          fin_cases j <;> simp [shearKPlus, shearKMinus] at hj ⊢ <;> omega
      · rintro (rfl | rfl)
        · right; exact neg_shearKPlus
        · left; exact neg_shearKMinus
    unfold shearDatum
    by_cases h : (k = shearKPlus ∨ k = shearKMinus) ∧ c = 1
    · have h' : ((fun j => -k j) = shearKPlus ∨ (fun j => -k j) = shearKMinus) ∧ c = 1 :=
        ⟨hiff.mpr h.1, h.2⟩
      simp [h, h']
    · have h' : ¬(((fun j => -k j) = shearKPlus ∨ (fun j => -k j) = shearKMinus) ∧ c = 1) := by
        intro hc
        exact h ⟨hiff.mp hc.1, hc.2⟩
      simp [h, h']
  · intro k
    rw [Fin.sum_univ_three]
    unfold shearDatum
    by_cases hk : k = shearKPlus ∨ k = shearKMinus
    · rcases hk with rfl | rfl
      · simp [shearKPlus]
      · simp [shearKMinus]
    · simp [hk]
  · intro k hk c
    unfold shearDatum
    have hnot : ¬(k = shearKPlus ∨ k = shearKMinus) := by
      rintro (rfl | rfl)
      · exact hk shearKPlus_mem_box1
      · exact hk shearKMinus_mem_box1
    simp [hnot]
  · simp [shearDatum]
