-- @lean-policy: 4
-- @depends-on: chain-bsd-2-fermat-noncongruent-1
import Mathlib.Data.Nat.Prime.Basic
import SwarmProofs.Chains.BSD.Defs

/-- Chain B [Rung 3/4 → BSD Congruent Numbers] (Heegner 1952 & Monsky 1990 Prime Slice):
    Every prime `p ≡ 5` or `7 (mod 8)` is a congruent number. Since no prime satisfies
    `p ≡ 6 (mod 8)`, this resolves the entire `ω(n) = 1` prime slice of the live Millennium
    bounty `millennium-bsd-congruent-squarefree`.
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∀ (p : ℕ), Nat.Prime p → (p % 8 = 5 ∨ p % 8 = 7) → isCongruentNumber p
