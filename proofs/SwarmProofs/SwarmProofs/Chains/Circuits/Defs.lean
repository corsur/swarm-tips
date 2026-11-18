/-
Copyright 2026 Swarm Tips contributors
SPDX-License-Identifier: Apache-2.0
-/

import Mathlib.Data.Fin.Basic
import Mathlib.Data.Fintype.Basic

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

/-- Evaluate a size-`s` straight-line circuit on an infinite wire assignment `x : ℕ → Bool`. -/
def evalCircuit (n s : ℕ) (gates : ℕ → Gate) (x : ℕ → Bool) : Bool :=
  (evalWires n gates x s) (n + s - 1)

/-- Evaluate a size-`s` circuit on a finite `Fin n → Bool` input vector. -/
def evalCircuitFin (n s : ℕ) (gates : ℕ → Gate) (x : Fin n → Bool) : Bool :=
  evalCircuit n s gates (fun i => if h : i < n then x ⟨i, h⟩ else false)

/-- n-bit Boolean parity x_0 ⊕ x_1 ⊕ ... ⊕ x_{n-1}. -/
def parityBool : ℕ → (ℕ → Bool) → Bool
  | 0, _ => false
  | m + 1, x => Bool.xor (parityBool m x) (x m)

/-- A 2-input Boolean gate belongs to the 14-operation De Morgan (canalizing / non-parity)
    basis iff it is neither `XOR` nor `XNOR`. -/
def isDeMorganOp (op : Bool → Bool → Bool) : Prop :=
  (∃ a b : Bool, op a b ≠ Bool.xor a b) ∧ (∃ a b : Bool, op a b ≠ !(Bool.xor a b))

/-- Canonical 3-gate De Morgan realization of 2-bit XOR (`OR`, `NAND`, `AND`),
    witnessing tightness of the `2n - 1 = 3` lower bound at `n = 2`. -/
def xor3DeMorganGates : ℕ → Gate
  | 0 => { in1 := 0, in2 := 1, op := fun a b => a || b }
  | 1 => { in1 := 0, in2 := 1, op := fun a b => !(a && b) }
  | _ => { in1 := 2, in2 := 3, op := fun a b => a && b }

/-- Helper encoding a 2-bit input `(b0, b1)` as `ℕ → Bool` for finite truth-table checks. -/
def mkInput2 (b0 b1 : Bool) : ℕ → Bool
  | 0 => b0
  | 1 => b1
  | _ => false

#guard evalCircuit 2 3 xor3DeMorganGates (mkInput2 false false) = false
#guard evalCircuit 2 3 xor3DeMorganGates (mkInput2 false true) = true
#guard evalCircuit 2 3 xor3DeMorganGates (mkInput2 true false) = true
#guard evalCircuit 2 3 xor3DeMorganGates (mkInput2 true true) = false
#guard evalCircuit 2 3 xor3DeMorganGates (mkInput2 false false) = parityBool 2 (mkInput2 false false)
#guard evalCircuit 2 3 xor3DeMorganGates (mkInput2 false true) = parityBool 2 (mkInput2 false true)
#guard evalCircuit 2 3 xor3DeMorganGates (mkInput2 true false) = parityBool 2 (mkInput2 true false)
#guard evalCircuit 2 3 xor3DeMorganGates (mkInput2 true true) = parityBool 2 (mkInput2 true true)
