/- SPDX-License-Identifier: Apache-2.0 -/
import Mathlib
import Zeta23

namespace SwarmProofs.Generated.Fixture
inductive Sneaky where
  | mk : Sneaky
  deriving DecidableEq
theorem proof : True := trivial
end SwarmProofs.Generated.Fixture
