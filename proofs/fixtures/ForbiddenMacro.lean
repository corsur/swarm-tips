/- SPDX-License-Identifier: Apache-2.0 -/
namespace SwarmProofs.Generated.Fixture
macro "fakeProof" : tactic => `(tactic| trivial)
theorem harmless : True := by fakeProof
end SwarmProofs.Generated.Fixture
