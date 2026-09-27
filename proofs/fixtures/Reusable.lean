/- SPDX-License-Identifier: Apache-2.0 -/
namespace SwarmProofs.Generated.Fixture
def twice (n : Nat) : Nat := n + n
private theorem helper (n : Nat) : twice n = n + n := rfl
theorem twice_eq (n : Nat) : twice n = n + n := helper n
-- Not referenced by the entry theorem; nevertheless independently checked.
def unusedDefinition : Nat := 7
end SwarmProofs.Generated.Fixture
