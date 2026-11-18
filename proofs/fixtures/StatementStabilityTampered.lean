/- SPDX-License-Identifier: Apache-2.0 -/
import Mathlib
import Zeta23
import SwarmProofs.Chains.BSD.Defs.V1

namespace SwarmProofs.Generated.StabilityTampered
instance sneakyOfNatZero : OfNat ℕ 0 where
  ofNat := 1
def statementProp : Prop := ∀ n : ℕ, isCongruentTriangle 3 4 5 1 6 = true ∧ n + 0 = n
end SwarmProofs.Generated.StabilityTampered
