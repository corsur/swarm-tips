import Mathlib.Data.Fin.Basic
import Mathlib.Data.Real.Basic
import Mathlib.Tactic.Linarith
import Mathlib.Tactic.NormNum
import Mathlib.Tactic.Ring

/-- Map a Boolean value to `{-1, +1} ⊂ ℝ`. -/
def boolSign (b : Bool) : ℝ :=
  if b then -1 else 1

/-- 1-bit Boolean function `h(x) = 1 + ε · boolSign (x 0)`. -/
def testFunc1 (ε : ℝ) (x : Fin 1 → Bool) : ℝ :=
  1 + ε * boolSign (x ⟨0, by decide⟩)

/-- 1-bit Bonami–Beckner noise operator `(T_ρ h)(x) = 1 + ρ ε · boolSign (x 0)` on `testFunc1`. -/
def testNoise1 (ρ ε : ℝ) (x : Fin 1 → Bool) : ℝ :=
  1 + ρ * ε * boolSign (x ⟨0, by decide⟩)

/-- Exact uniform expectation `(h(false) + h(true)) / 2` on `{0, 1}^1`. -/
noncomputable def expect1 (h : (Fin 1 → Bool) → ℝ) : ℝ :=
  (h (fun _ => false) + h (fun _ => true)) / 2

/-- Unbounded-fanin `AC^0` formula tree over `n` Boolean variables. -/
inductive AC0Formula (n : ℕ) : Type where
  | const : Bool → AC0Formula n
  | lit : Fin n → Bool → AC0Formula n
  | andList : List (AC0Formula n) → AC0Formula n
  | orList : List (AC0Formula n) → AC0Formula n

mutual
  def evalAC0 {n : ℕ} (x : Fin n → Bool) : AC0Formula n → Bool
    | .const b => b
    | .lit i neg => if neg then !(x i) else x i
    | .andList cs => evalAC0And x cs
    | .orList cs => evalAC0Or x cs

  def evalAC0And {n : ℕ} (x : Fin n → Bool) : List (AC0Formula n) → Bool
    | [] => true
    | c :: cs => evalAC0 x c && evalAC0And x cs

  def evalAC0Or {n : ℕ} (x : Fin n → Bool) : List (AC0Formula n) → Bool
    | [] => false
    | c :: cs => evalAC0 x c || evalAC0Or x cs
end

mutual
  def ac0Depth {n : ℕ} : AC0Formula n → ℕ
    | .const _ => 0
    | .lit _ _ => 0
    | .andList cs => 1 + ac0DepthList cs
    | .orList cs => 1 + ac0DepthList cs

  def ac0DepthList {n : ℕ} : List (AC0Formula n) → ℕ
    | [] => 0
    | c :: cs => max (ac0Depth c) (ac0DepthList cs)
end

mutual
  def ac0Size {n : ℕ} : AC0Formula n → ℕ
    | .const _ => 1
    | .lit _ _ => 1
    | .andList cs => 1 + ac0SizeList cs
    | .orList cs => 1 + ac0SizeList cs

  def ac0SizeList {n : ℕ} : List (AC0Formula n) → ℕ
    | [] => 0
    | c :: cs => ac0Size c + ac0SizeList cs
end

/-- Exact `n`-bit parity via fold over `List.finRange n`. -/
def parityFin (n : ℕ) (x : Fin n → Bool) : Bool :=
  (List.finRange n).foldl (fun acc i => Bool.xor acc (x i)) false

def parity2DNF : AC0Formula 2 :=
  .orList
    [ .andList [.lit ⟨0, by decide⟩ false, .lit ⟨1, by decide⟩ true]
    , .andList [.lit ⟨0, by decide⟩ true, .lit ⟨1, by decide⟩ false] ]

/-- Master Synthesis & Sharpness Theorem (Chain F — Boolean Fourier Analysis):
    (1) Exact sharpness of Bonami's `(2,4)` hypercontractive threshold `ρ^2 ≤ 1/3` in Rung F1:
        for EVERY `ρ : ℝ` with `1/3 < ρ^2`, there exists `ε > 0` such that the 1-bit function
        `h(x) = 1 + ε · boolSign(x_0)` strictly violates `E[(T_ρ h)^4] > (E[h^2])^2`;
    (2) Exact depth-2 DNF witness for 2-bit parity (`ac0Depth = 2`, `ac0Size = 7`, exact evaluation
        on all 4 inputs `x : Fin 2 → Bool`). -/
def statementProp : Prop :=
  (∀ ρ : ℝ, (1 : ℝ) / 3 < ρ ^ 2 →
    ∃ ε : ℝ, 0 < ε ∧
      (expect1 (fun x => (testFunc1 ε x) ^ 2)) ^ 2 <
        expect1 (fun x => (testNoise1 ρ ε x) ^ 4)) ∧
  (ac0Depth parity2DNF = 2 ∧
    ac0Size parity2DNF = 7 ∧
    ∀ x : Fin 2 → Bool, evalAC0 x parity2DNF = parityFin 2 x)

theorem proof : statementProp := by
  constructor
  · intro ρ hρ
    have hδ : 0 < 6 * ρ ^ 2 - 2 := by linarith
    let ε : ℝ := min (1 / 2 : ℝ) ((6 * ρ ^ 2 - 2) / 2)
    have hε_pos : 0 < ε := lt_min (by norm_num) (by linarith)
    have hε_le1 : ε ≤ 1 / 2 := min_le_left _ _
    have hε_leδ : ε ≤ (6 * ρ ^ 2 - 2) / 2 := min_le_right _ _
    refine ⟨ε, hε_pos, ?_⟩
    simp only [expect1, testFunc1, testNoise1, boolSign, ↓reduceIte, Bool.false_eq_true]
    have hε2_le : ε ^ 2 ≤ ε := by nlinarith
    have hρ4 : 0 ≤ ρ ^ 4 := by nlinarith [sq_nonneg ρ]
    have hinner : 0 < (6 * ρ ^ 2 - 2) + (ρ ^ 4 - 1) * ε ^ 2 := by nlinarith
    have hdiff :
        (((1 + ρ * ε * 1) ^ 4 + (1 + ρ * ε * -1) ^ 4) / 2) -
        ((((1 + ε * 1) ^ 2 + (1 + ε * -1) ^ 2) / 2) ^ 2) =
        ε ^ 2 * ((6 * ρ ^ 2 - 2) + (ρ ^ 4 - 1) * ε ^ 2) := by ring
    have hmul : 0 < ε ^ 2 * ((6 * ρ ^ 2 - 2) + (ρ ^ 4 - 1) * ε ^ 2) :=
      mul_pos (sq_pos_of_pos hε_pos) hinner
    linarith
  · refine ⟨rfl, rfl, ?_⟩
    intro x
    change
      ((x ⟨0, by decide⟩ && (!(x ⟨1, by decide⟩) && true)) ||
        ((!(x ⟨0, by decide⟩) && (x ⟨1, by decide⟩ && true)) || false)) =
      Bool.xor (Bool.xor false (x ⟨0, by decide⟩)) (x ⟨1, by decide⟩)
    cases x ⟨0, by decide⟩ <;> cases x ⟨1, by decide⟩ <;> rfl
