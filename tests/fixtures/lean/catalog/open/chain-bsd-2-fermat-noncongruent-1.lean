-- @lean-policy: 4
-- @depends-on: chain-bsd-1-weierstrass-bijection
import Mathlib.Data.Nat.Basic
import SwarmProofs.Chains.BSD.Defs.V1

/-- Chain B [Rung 2/4 → BSD Congruent Numbers] (Fermat's Right-Triangle Theorem):
    `1` is not a congruent number (`¬ isCongruentNumber 1`), proved classically by infinite
    descent on `x⁴ - y⁴ = z²`. Via Rung B1, this establishes Mordell–Weil rank 0 on
    `E_1 : y² = x³ - x` where `L(E_1, 1) ≠ 0` (the rank-0 base case of BSD).
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ¬ isCongruentNumber 1
