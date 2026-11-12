import Mathlib.Data.Fin.Basic
import Mathlib.Data.Fintype.Basic
import Mathlib.Tactic.IntervalCases
import Mathlib.Tactic.Linarith
import Mathlib.Tactic.Ring

/-- A single 2-input Boolean gate over `Bool → Bool → Bool`. -/
structure Gate where
  in1 : ℕ
  in2 : ℕ
  op : Bool → Bool → Bool

/-- Evaluate a straight-line DAG circuit of `s` gates on `x : ℕ → Bool`. -/
def evalWires (n : ℕ) (gates : ℕ → Gate) (x : ℕ → Bool) : ℕ → (ℕ → Bool)
  | 0 => x
  | k + 1 =>
      let prev := evalWires n gates x k
      let g := gates k
      let w1 := prev (g.in1 % (n + k))
      let w2 := prev (g.in2 % (n + k))
      fun i => if i = n + k then g.op w1 w2 else prev i

def evalCircuit (n s : ℕ) (gates : ℕ → Gate) (x : ℕ → Bool) : Bool :=
  (evalWires n gates x s) (n + s - 1)

/-- Evaluate a size-`s` circuit on a finite `Fin n → Bool` input vector. -/
def evalCircuitFin (n s : ℕ) (gates : ℕ → Gate) (x : Fin n → Bool) : Bool :=
  evalCircuit n s gates (fun i => if h : i < n then x ⟨i, h⟩ else false)

/-- n-bit Boolean parity x_0 ⊕ x_1 ⊕ ... ⊕ x_{n-1}. -/
def parityBool : ℕ → (ℕ → Bool) → Bool
  | 0, _ => false
  | m + 1, x => Bool.xor (parityBool m x) (x m)

/-- A 2-input Boolean gate belongs to the 14-op De Morgan (non-parity) basis iff
    it is neither `XOR` nor `XNOR`. -/
def isDeMorganOp (op : Bool → Bool → Bool) : Prop :=
  (∃ a b : Bool, op a b ≠ Bool.xor a b) ∧ (∃ a b : Bool, op a b ≠ !(Bool.xor a b))

/-- Rung D1: De Morgan Parity Gate-Elimination Lower Bound (s ≥ 2n - 1 for all n ≥ 2). -/
def RungD1 : Prop :=
  ∀ (n : ℕ), 2 ≤ n →
    ∀ (s : ℕ), s < 2 * n - 1 →
      ∀ (gates : ℕ → Gate),
        (∀ k < s, isDeMorganOp (gates k).op) →
          ∃ (x : ℕ → Bool), evalCircuit n s gates x ≠ parityBool n x

/-- Rung D2: Shannon (1949) Exponential Circuit Counting Bound over full B_2. -/
def RungD2 : Prop :=
  ∀ (n : ℕ), 6 ≤ n →
    ∃ (f : (Fin n → Bool) → Bool),
      ∀ (s : ℕ), 1 ≤ s → 4 * n * s ≤ 2 ^ n →
        ∀ (gates : ℕ → Gate),
          ∃ (x : Fin n → Bool), evalCircuitFin n s gates x ≠ f x

/-- Explicit 3-gate De Morgan realization of 2-bit XOR (`OR`, `NAND`, `AND`),
    witnessing that Rung D1's bound `2n - 1 = 3` at `n = 2` is tight. -/
def xor3Gates : ℕ → Gate
  | 0 => { in1 := 0, in2 := 1, op := fun a b => a || b }
  | 1 => { in1 := 0, in2 := 1, op := fun a b => !(a && b) }
  | _ => { in1 := 2, in2 := 3, op := fun a b => a && b }

/-- Master Synthesis Theorem (Track D):
    (1) `xor3Gates` proves `2n - 1` in Rung D1 is sharp at `n = 2` using pure De Morgan gates;
    (2) `RungD2` implies that for EVERY linear multiplier `k ≥ 1`, there exist `n ≥ 6` and a
        hard function `f : (Fin n → Bool) → Bool` defeating ALL `B_2` circuits of size `≤ k · n`. -/
def statementProp : Prop :=
  ((∀ k < 3, isDeMorganOp (xor3Gates k).op) ∧
    (∀ x : ℕ → Bool, evalCircuit 2 3 xor3Gates x = parityBool 2 x)) ∧
  (RungD2 →
    ∀ (k : ℕ), 1 ≤ k →
      ∃ (n : ℕ), 6 ≤ n ∧
        ∃ (f : (Fin n → Bool) → Bool),
          ∀ (s : ℕ), 1 ≤ s → s ≤ k * n →
            ∀ (gates : ℕ → Gate),
              ∃ (x : Fin n → Bool), evalCircuitFin n s gates x ≠ f x)

lemma cube_lt_pow_two : ∀ m : ℕ, 10 ≤ m → m ^ 3 < 2 ^ m := by
  intro m hm
  induction m, hm using Nat.le_induction with
  | base => decide
  | succ m hm ih =>
    have hpoly : (m + 1) ^ 3 < 2 * (m ^ 3) := by
      have hm2 : 100 ≤ m * m := Nat.mul_le_mul hm hm
      nlinarith
    calc
      (m + 1) ^ 3 < 2 * (m ^ 3) := hpoly
      _ < 2 * (2 ^ m) := Nat.mul_lt_mul_of_pos_left ih (by decide)
      _ = 2 ^ (m + 1) := by ring

theorem proof : statementProp := by
  constructor
  · constructor
    · intro k hk
      interval_cases k
      · exact ⟨⟨true, true, by decide⟩, ⟨false, false, by decide⟩⟩
      · exact ⟨⟨false, false, by decide⟩, ⟨false, true, by decide⟩⟩
      · exact ⟨⟨true, false, by decide⟩, ⟨false, false, by decide⟩⟩
    · intro x
      cases h0 : x 0 <;> cases h1 : x 1 <;>
        simp [evalCircuit, evalWires, xor3Gates, parityBool, h0, h1]
  · intro hD2 k hk1
    let n : ℕ := 4 * k + 10
    have hn6 : 6 ≤ n := by omega
    have hn10 : 10 ≤ n := by omega
    obtain ⟨f, hf⟩ := hD2 n hn6
    refine ⟨n, hn6, f, ?_⟩
    intro s hs1 hsk gates
    have h4ns : 4 * n * s ≤ 2 ^ n := by
      have h1 : 4 * n * s ≤ 4 * n * (k * n) := Nat.mul_le_mul_left (4 * n) hsk
      have h4kle : 4 * k ≤ n := by omega
      have h2 : 4 * n * (k * n) ≤ n ^ 3 := by
        calc
          4 * n * (k * n) = (4 * k) * (n * n) := by ring
          _ ≤ n * (n * n) := Nat.mul_le_mul_right (n * n) h4kle
          _ = n ^ 3 := by ring
      exact le_trans h1 (le_trans h2 (Nat.le_of_lt (cube_lt_pow_two n hn10)))
    exact hf s hs1 h4ns gates
