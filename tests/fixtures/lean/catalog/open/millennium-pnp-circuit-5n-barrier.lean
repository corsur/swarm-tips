import Mathlib.Data.Fin.Basic
import Mathlib.Data.Fintype.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic

open scoped BigOperators

/-- A single 2-input Boolean gate: selects two prior wire indices `in1, in2 : ℕ`
    (interpreted modulo the number of currently available wires `n + k`) and applies
    an arbitrary binary Boolean truth table `op : Bool → Bool → Bool`. -/
structure Gate where
  in1 : ℕ
  in2 : ℕ
  op : Bool → Bool → Bool

/-- Evaluate a straight-line DAG circuit of `s` gates on `n` Boolean inputs `x : ℕ → Bool`.
    Gate `k` reads two strictly earlier wires `(g.in1 % (n + k))` and `(g.in2 % (n + k))`
    and writes wire `n + k`. The circuit output is the final wire `n + s - 1`. -/
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

/-- Explicit polynomial-time target Boolean function `mod3Quadratic n x`: returns `true` iff
    the Hamming weight ∑_{i < n} x_i and cyclic pairwise AND ∑_{i < n} (x_i && x_{(i+1) mod n})
    satisfy `(weight + 2 * pairs) ≡ 1 (mod 3)`. -/
def mod3Quadratic (n : ℕ) (x : ℕ → Bool) : Bool :=
  let w := ∑ i ∈ Finset.range n, if x i then 1 else 0
  let p := ∑ i ∈ Finset.range n, if x i && x ((i + 1) % n) then 1 else 0
  ((w + 2 * p) % 3) == 1

/-- Millennium Problem (P vs NP — Breaking the 5n General Boolean Circuit Size Barrier):
    By Cook–Levin (1971) and Savage (1972), P ≠ NP follows from super-polynomial circuit
    size lower bounds for NP predicates, yet the strongest unconditional lower bound known
    over the full 2-input basis B_2 is 3.01n (Find–Golovnev–Hirsch–Kulikov 2016 / Li–Yang 2022).
    This open problem asks whether every 2-input Boolean circuit computing `mod3Quadratic n`
    for all n ≥ 16 requires strictly more than 5n gates. -/
def statementProp : Prop :=
  ∀ (n : ℕ), 16 ≤ n →
    ∀ (s : ℕ), 1 ≤ s → s ≤ 5 * n →
      ∀ (gates : ℕ → Gate),
        ∃ (x : ℕ → Bool),
          evalCircuit n s gates x ≠ mod3Quadratic n x
