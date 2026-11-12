import Mathlib.Data.Nat.Basic

/-- A single 2-input Boolean gate over `Bool → Bool → Bool`. -/
structure Gate where
  in1 : ℕ
  in2 : ℕ
  op : Bool → Bool → Bool

/-- Evaluate a straight-line DAG circuit of `s` gates on `x : ℕ → Bool`.
    Gate `k` reads two earlier wires `(g.in1 % (n + k))` and `(g.in2 % (n + k))`
    and writes wire `n + k`. The circuit output is wire `n + s - 1`. -/
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

/-- n-bit Boolean parity x_0 ⊕ x_1 ⊕ ... ⊕ x_{n-1}. -/
def parityBool : ℕ → (ℕ → Bool) → Bool
  | 0, _ => false
  | m + 1, x => Bool.xor (parityBool m x) (x m)

/-- A 2-input Boolean gate belongs to the 14-operation De Morgan (canalizing / non-parity)
    basis iff it is neither `XOR` nor `XNOR`. -/
def isDeMorganOp (op : Bool → Bool → Bool) : Prop :=
  (∃ a b : Bool, op a b ≠ Bool.xor a b) ∧ (∃ a b : Bool, op a b ≠ !(Bool.xor a b))

/-- Track D [Rung 1/2 → Classical Boolean Circuit Lower Bounds] (Schnorr 1974 / Red'kin 1973):
    Unconditionally provable classical circuit lower bound: for every `n ≥ 2`, every straight-line
    circuit of size `s < 2n - 1` restricted to the 14-op De Morgan basis (`op ≠ XOR, XNOR`) fails
    to compute `n`-bit parity on at least one input `x`. Sharp at `n = 2` (`s = 3` achieved by
    `OR + NAND + AND` in `proofs/millennium-chains/pnp_circuit_foundations_chain.lean`). -/
def statementProp : Prop :=
  ∀ (n : ℕ), 2 ≤ n →
    ∀ (s : ℕ), s < 2 * n - 1 →
      ∀ (gates : ℕ → Gate),
        (∀ k < s, isDeMorganOp (gates k).op) →
          ∃ (x : ℕ → Bool), evalCircuit n s gates x ≠ parityBool n x
