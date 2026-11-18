-- @lean-policy: 4
-- @depends-on: chain-pnp-circuit-1-de-morgan-parity-lower-bound
import Mathlib.Data.Fin.Basic
import Mathlib.Data.Fintype.Basic
import SwarmProofs.Chains.Circuits.Defs.V1

/-- Track D [Rung 2/2 → Classical Boolean Circuit Lower Bounds] (Shannon 1949 Counting Bound):
    Unconditionally provable classical circuit lower bound over the full 16-operation basis `B_2`:
    for every `n ≥ 6`, there exists an `n`-bit Boolean function `f : (Fin n → Bool) → Bool` that
    cannot be computed by ANY 2-input Boolean circuit of size `1 ≤ s ≤ ⌊2^n / (4n)⌋`
    (since `(16 · (n + s)^2)^s < 2^{2n s} < 2^{2^n}`).
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∀ (n : ℕ), 6 ≤ n →
    ∃ (f : (Fin n → Bool) → Bool),
      ∀ (s : ℕ), 1 ≤ s → 4 * n * s ≤ 2 ^ n →
        ∀ (gates : ℕ → Gate),
          ∃ (x : Fin n → Bool), evalCircuitFin n s gates x ≠ f x
