-- @lean-policy: 4
-- @depends-on: chain-bsd-3-heegner-monsky-primes
import Mathlib.Data.Nat.Prime.Basic
import SwarmProofs.Chains.BSD.Defs.V1

/-- Chain B [Rung 4/4 → BSD Congruent Numbers] (Open Semiprime BSD Slice, ω(n) = 2):
    Every semiprime `n = p · q` (`p < q` prime) with `(p · q) ≡ 5, 6, 7 (mod 8)` is a
    congruent number. Isolates the exact first open Selmer-rank ≥ 3 frontier (`p ≡ 1 mod 8`,
    `q ≡ 5 mod 8`, Legendre symbol `(p/q) = +1`) of `millennium-bsd-congruent-squarefree`.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∀ (p q : ℕ), Nat.Prime p → Nat.Prime q → p < q →
    ((p * q) % 8 = 5 ∨ (p * q) % 8 = 6 ∨ (p * q) % 8 = 7) →
      isCongruentNumber (p * q)
