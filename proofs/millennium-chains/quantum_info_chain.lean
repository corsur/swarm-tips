import Mathlib.Data.Fin.Basic
import Mathlib.Data.Int.Basic
import Mathlib.Data.Real.Basic
import Mathlib.Tactic.Linarith
import Mathlib.Tactic.NormNum
import Mathlib.Tactic.Ring

/-- Classical ±1 sign mapping for deterministic CHSH strategies. -/
def signInt (b : Bool) : ℤ :=
  if b then -1 else 1

/-- Classical CHSH correlation value `a0 b0 + a0 b1 + a1 b0 - a1 b1`. -/
def classicalChshValue (a0 a1 b0 b1 : Bool) : ℤ :=
  signInt a0 * signInt b0 +
  signInt a0 * signInt b1 +
  signInt a1 * signInt b0 -
  signInt a1 * signInt b1

/-- Abstract single-step detectability contraction hypothesis on an orthogonal norm sequence
    `normSq : ℕ → ℝ` (`‖D^k v‖²`), matching Rung G3 (`Chain G` Detectability Lemma). -/
def DetectabilityStep (normSq : ℕ → ℝ) (g : ℕ) (Δ : ℝ) : Prop :=
  0 < Δ ∧
  (∀ k : ℕ, 0 ≤ normSq k) ∧
  ∀ k : ℕ, normSq (k + 1) ≤ (((g : ℝ) ^ 2) / ((g : ℝ) ^ 2 + Δ)) * normSq k

/-- Master Synthesis & Sharpness Theorem (Chain G — Quantum Information & Hamiltonian Complexity):
    (1) Exact Bell / CHSH classical ceiling: EVERY deterministic local hidden-variable strategy
        satisfies `|classicalChshValue a0 a1 b0 b1| ≤ 2`, with sharp attainment `= 2`;
    (2) Strict Bell violation margin: any quantum state achieving Tsirelson's value `T` (`T^2 = 8`,
        `T > 0`, as established in Rung G1) strictly exceeds the classical ceiling (`2 < T`);
    (3) Detectability Lemma exponential amplification (from Rung G3): single-step contraction
        `‖D^{k+1} v‖² ≤ (g² / (g² + Δ)) ‖D^k v‖²` implies exponential decay
        `‖D^k v‖² ≤ (g² / (g² + Δ))^k ‖v‖²` for every `k : ℕ`. -/
def statementProp : Prop :=
  ((∀ a0 a1 b0 b1 : Bool,
      (-2 : ℤ) ≤ classicalChshValue a0 a1 b0 b1 ∧
      classicalChshValue a0 a1 b0 b1 ≤ 2) ∧
    (∃ a0 a1 b0 b1 : Bool, classicalChshValue a0 a1 b0 b1 = 2)) ∧
  (∀ T : ℝ, 0 < T → T ^ 2 = 8 → (2 : ℝ) < T) ∧
  (∀ (normSq : ℕ → ℝ) (g : ℕ) (Δ : ℝ),
    DetectabilityStep normSq g Δ →
    ∀ k : ℕ, normSq k ≤ (((g : ℝ) ^ 2) / ((g : ℝ) ^ 2 + Δ)) ^ k * normSq 0)

theorem proof : statementProp := by
  refine ⟨⟨?_, ?_⟩, ?_, ?_⟩
  · intro a0 a1 b0 b1
    cases a0 <;> cases a1 <;> cases b0 <;> cases b1 <;> decide
  · exact ⟨false, false, false, false, rfl⟩
  · intro T hTpos hTsq
    nlinarith
  · intro normSq g Δ ⟨hΔ, hnn, hstep⟩ k
    have hdenom : 0 < (g : ℝ) ^ 2 + Δ := by nlinarith [sq_nonneg (g : ℝ)]
    have hq_nonneg : 0 ≤ ((g : ℝ) ^ 2) / ((g : ℝ) ^ 2 + Δ) :=
      div_nonneg (sq_nonneg _) (le_of_lt hdenom)
    induction k with
    | zero =>
      simp
    | succ k ih =>
      calc
        normSq (k + 1) ≤ (((g : ℝ) ^ 2) / ((g : ℝ) ^ 2 + Δ)) * normSq k := hstep k
        _ ≤ (((g : ℝ) ^ 2) / ((g : ℝ) ^ 2 + Δ)) *
              ((((g : ℝ) ^ 2) / ((g : ℝ) ^ 2 + Δ)) ^ k * normSq 0) :=
          mul_le_mul_of_nonneg_left ih hq_nonneg
        _ = (((g : ℝ) ^ 2) / ((g : ℝ) ^ 2 + Δ)) ^ (k + 1) * normSq 0 := by ring
