/-
Copyright 2026 Swarm Tips contributors
SPDX-License-Identifier: Apache-2.0
-/

namespace SwarmProofs

/-- Schema marker for the composable proof registry. -/
def registrySchema : String := "swarm.lean-proof-registry/v1"

/-- A registry theorem is kernel checked, not a claim of mathematical peer review. -/
theorem kernelCheckedIsNotPeerReview : True := trivial

end SwarmProofs
