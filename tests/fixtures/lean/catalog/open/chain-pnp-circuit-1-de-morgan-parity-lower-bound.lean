-- @lean-policy: 4
import Mathlib.Data.Nat.Basic
import SwarmProofs.Chains.Circuits.Defs.V1

/-- Track D [Rung 1/2 → Classical Boolean Circuit Lower Bounds] (Schnorr 1974 / Red'kin 1973):
    Unconditionally provable classical circuit lower bound: for every `n ≥ 2`, every straight-line
    circuit of size `s < 2n - 1` restricted to the 14-op De Morgan basis (`op ≠ XOR, XNOR`) fails
    to compute `n`-bit parity on at least one input `x`. Sharp at `n = 2` (`s = 3` achieved by
    `OR + NAND + AND` in `proofs/millennium-chains/pnp_circuit_foundations_chain.lean`).
    Solver proofs must start with `-- SPDX-License-Identifier: Apache-2.0` for registry promotion. -/
def statementProp : Prop :=
  ∀ (n : ℕ), 2 ≤ n →
    ∀ (s : ℕ), s < 2 * n - 1 →
      ∀ (gates : ℕ → Gate),
        (∀ k < s, isDeMorganOp (gates k).op) →
          ∃ (x : ℕ → Bool), evalCircuit n s gates x ≠ parityBool n x
