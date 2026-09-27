/- SPDX-License-Identifier: Apache-2.0 -/
import Reusable

namespace SwarmProofs.Generated.Consumer
open SwarmProofs.Generated.Fixture

def fourfold (n : Nat) : Nat := twice (twice n)

theorem fourfold_eq (n : Nat) : fourfold n = (n + n) + (n + n) := by
  simp only [fourfold, twice_eq]

end SwarmProofs.Generated.Consumer
