/-
Copyright 2026 Swarm Tips contributors
SPDX-License-Identifier: Apache-2.0
-/

import Mathlib.Data.Fin.Basic
import Mathlib.Data.Finset.Basic
import Mathlib.Data.Fintype.BigOperators
import Mathlib.Data.Fintype.Pi
import Mathlib.Data.Fintype.Powerset
import Mathlib.Data.Real.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic

/-- Map a Boolean value to `{-1, +1} ⊂ ℝ` (`false ↦ +1`, `true ↦ -1`),
    matching the standard Walsh–Fourier convention `χ_S(x) = (-1)^{∑_{i ∈ S} x_i}`. -/
def boolSign (b : Bool) : ℝ :=
  if b then -1 else 1

/-- Rational sign helper for computable `#guard` sanity checks. -/
def boolSignRat (b : Bool) : ℚ :=
  if b then -1 else 1

/-- Walsh–Fourier parity character `χ_S(x) = ∏_{i ∈ S} (-1)^{x_i}` on `{0,1}^n`. -/
def chi (n : ℕ) (S : Finset (Fin n)) (x : Fin n → Bool) : ℝ :=
  Finset.prod S (fun i => boolSign (x i))

/-- Uniform expectation `E_{x ∈ {0,1}^n}[f(x)] = 2^{-n} ∑_x f(x)` over the Boolean hypercube. -/
noncomputable def boolExpect (n : ℕ) (f : (Fin n → Bool) → ℝ) : ℝ :=
  (Finset.sum Finset.univ f) / ((2 : ℝ) ^ n)

/-- Walsh–Fourier coefficient `f̂(S) = E_x[f(x) · χ_S(x)]`. -/
noncomputable def walshFourierCoeff (n : ℕ) (f : (Fin n → Bool) → ℝ) (S : Finset (Fin n)) : ℝ :=
  boolExpect n (fun x => f x * chi n S x)

/-- Flip the `i`-th coordinate of an `n`-bit Boolean input vector. -/
def flipBit (n : ℕ) (x : Fin n → Bool) (i : Fin n) : Fin n → Bool :=
  fun j => if j = i then !x j else x j

/-- Coordinate influence `Inf_i(f) = Pr_x[f(x) ≠ f(x^{⊕ i})]` for a Boolean function. -/
noncomputable def coordInfluence (n : ℕ) (f : (Fin n → Bool) → Bool) (i : Fin n) : ℝ :=
  boolExpect n (fun x => if f x ≠ f (flipBit n x i) then (1 : ℝ) else 0)

/-- Total influence (average sensitivity) `I(f) = ∑_{i=1}^n Inf_i(f)`. -/
noncomputable def totalInfluence (n : ℕ) (f : (Fin n → Bool) → Bool) : ℝ :=
  Finset.sum Finset.univ (fun i => coordInfluence n f i)

/-- Variance `Var(f) = E[(f - E[f])^2]` on the Boolean hypercube. -/
noncomputable def boolVariance (n : ℕ) (f : (Fin n → Bool) → ℝ) : ℝ :=
  boolExpect n (fun x => (f x - boolExpect n f) ^ 2)

/-- Bonami–Beckner noise operator `(T_ρ f)(x) = ∑_S ρ^{|S|} f̂(S) χ_S(x)`. -/
noncomputable def noiseOp (n : ℕ) (ρ : ℝ) (f : (Fin n → Bool) → ℝ) (x : Fin n → Bool) : ℝ :=
  Finset.sum Finset.univ (fun S : Finset (Fin n) => (ρ ^ S.card) * walshFourierCoeff n f S * chi n S x)

/-- Noise stability `Stab_ρ(f) = E_x[f(x) · (T_ρ f)(x)]`. -/
noncomputable def noiseStability (n : ℕ) (ρ : ℝ) (f : (Fin n → Bool) → ℝ) : ℝ :=
  boolExpect n (fun x => f x * noiseOp n ρ f x)

/-- Exact `n`-bit parity function on `Fin n → Bool` (`true` iff an odd number of bits are `true`). -/
def parityFin (n : ℕ) (x : Fin n → Bool) : Bool :=
  ((Finset.univ.filter (fun i : Fin n => x i = true)).card % 2) == 1

/-- Unbounded-fanin De Morgan (`AC^0`) formula tree over `n` Boolean variables. -/
inductive AC0Formula (n : ℕ) : Type where
  | const : Bool → AC0Formula n
  | lit : Fin n → Bool → AC0Formula n
  | andList : List (AC0Formula n) → AC0Formula n
  | orList : List (AC0Formula n) → AC0Formula n

mutual
  /-- Evaluate an unbounded-fanin `AC^0` formula on `x : Fin n → Bool`. -/
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
  /-- Alternation/gate depth of an `AC^0` formula (leaves have depth `0`). -/
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
  /-- Total number of nodes (gates + leaves) in an `AC^0` formula. -/
  def ac0Size {n : ℕ} : AC0Formula n → ℕ
    | .const _ => 1
    | .lit _ _ => 1
    | .andList cs => 1 + ac0SizeList cs
    | .orList cs => 1 + ac0SizeList cs

  def ac0SizeList {n : ℕ} : List (AC0Formula n) → ℕ
    | [] => 0
    | c :: cs => ac0Size c + ac0SizeList cs
end

/-- Computable rational expectation helper for `#guard` verification. -/
def boolExpectRat (n : ℕ) (f : (Fin n → Bool) → ℚ) : ℚ :=
  (Finset.sum Finset.univ f) / ((2 : ℚ) ^ n)

/-- Computable rational coordinate influence helper for `#guard` verification. -/
def coordInfluenceRat (n : ℕ) (f : (Fin n → Bool) → Bool) (i : Fin n) : ℚ :=
  boolExpectRat n (fun x => if f x ≠ f (flipBit n x i) then (1 : ℚ) else 0)

/-- Depth-2 DNF realizing 2-bit parity `(x0 ∧ ¬x1) ∨ (¬x0 ∧ x1)` of size 7. -/
def parity2DNF : AC0Formula 2 :=
  .orList
    [ .andList [.lit ⟨0, by decide⟩ false, .lit ⟨1, by decide⟩ true]
    , .andList [.lit ⟨0, by decide⟩ true, .lit ⟨1, by decide⟩ false] ]

#guard boolSignRat false = 1
#guard boolSignRat true = -1
#guard parityFin 2 (fun _ => false) = false
#guard parityFin 2 (fun i => i.val == 0) = true
#guard parityFin 2 (fun _ => true) = false
#guard coordInfluenceRat 2 (parityFin 2) ⟨0, by decide⟩ = 1
#guard coordInfluenceRat 2 (parityFin 2) ⟨1, by decide⟩ = 1
#guard ac0Depth parity2DNF = 2
#guard ac0Size parity2DNF = 7
#guard evalAC0 (fun _ => false) parity2DNF = parityFin 2 (fun _ => false)
#guard evalAC0 (fun i => i.val == 0) parity2DNF = parityFin 2 (fun i => i.val == 0)
#guard evalAC0 (fun i => i.val == 1) parity2DNF = parityFin 2 (fun i => i.val == 1)
#guard evalAC0 (fun _ => true) parity2DNF = parityFin 2 (fun _ => true)
