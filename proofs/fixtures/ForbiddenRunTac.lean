/- SPDX-License-Identifier: Apache-2.0 -/
namespace SwarmProofs.Generated.Fixture
theorem harmless : True := by
  run_tac Lean.logInfo "THIS MUST NEVER EXECUTE"
  trivial
end SwarmProofs.Generated.Fixture
