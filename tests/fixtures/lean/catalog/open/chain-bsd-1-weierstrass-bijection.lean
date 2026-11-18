-- @lean-policy: 4
import Mathlib.Data.Nat.Basic
import Mathlib.Data.Rat.Defs
import Mathlib.Tactic.Ring
import SwarmProofs.Chains.BSD.Defs

/-- Chain B [Rung 1/4 → BSD Congruent Numbers] (Koblitz Prop. I.2 Weierstrass Bijection):
    For every integer `n ≥ 1`, `n` is the area of a rational right triangle (`isCongruentNumber n`)
    if and only if the congruent-number elliptic curve `E_n : y² = x³ - n²x` has a rational
    point `(x, y) ∈ ℚ × ℚ` outside the 2-torsion locus `y = 0`.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∀ (n : ℕ), 1 ≤ n →
    (isCongruentNumber n ↔ ∃ (x y : ℚ), y ≠ 0 ∧ y ^ 2 = x ^ 3 - (n : ℚ) ^ 2 * x)
