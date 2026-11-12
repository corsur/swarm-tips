import Mathlib.Data.Fin.Basic
import Mathlib.Data.Fintype.Basic

/-- A single 2-input Boolean gate over the full 16-operation basis `Bool → Bool → Bool`. -/
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

/-- Track D [Rung 2/2 → Classical Boolean Circuit Lower Bounds] (Shannon 1949 Counting Bound):
    Unconditionally provable classical circuit lower bound over the full 16-operation basis `B_2`:
    for every `n ≥ 6`, there exists an `n`-bit Boolean function `f : (Fin n → Bool) → Bool` that
    cannot be computed by ANY 2-input Boolean circuit of size `1 ≤ s ≤ ⌊2^n / (4n)⌋`
    (since `(16 · (n + s)^2)^s < 2^{2n s} < 2^{2^n}`). -/
def statementProp : Prop :=
  ∀ (n : ℕ), 6 ≤ n →
    ∃ (f : (Fin n → Bool) → Bool),
      ∀ (s : ℕ), 1 ≤ s → 4 * n * s ≤ 2 ^ n →
        ∀ (gates : ℕ → Gate),
          ∃ (x : Fin n → Bool), evalCircuitFin n s gates x ≠ f x
