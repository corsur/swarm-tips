/-
Copyright 2026 Swarm Tips contributors
SPDX-License-Identifier: Apache-2.0
-/

import Mathlib.Data.Nat.Basic
import Mathlib.Data.Rat.Defs

/-- Computable Boolean check witnessing `isCongruentNumber n` at `(a, b, c, d)`
    representing rational right-triangle sides `(a/d, b/d, c/d)` with area `n`. -/
def isCongruentTriangle (a b c d n : ℕ) : Bool :=
  decide (0 < a ∧ 0 < b ∧ 0 < c ∧ 0 < d ∧ a ^ 2 + b ^ 2 = c ^ 2 ∧ a * b = 2 * n * (d ^ 2))

/-- A positive integer `n` is a congruent number if `ab = 2nd²` and `a² + b² = c²`
    for positive integers `a, b, c, d` (representing rational sides `a/d, b/d, c/d`). -/
def isCongruentNumber (n : ℕ) : Prop :=
  ∃ (a b c d : ℕ), 0 < a ∧ 0 < b ∧ 0 < c ∧ 0 < d ∧
    a ^ 2 + b ^ 2 = c ^ 2 ∧ a * b = 2 * n * (d ^ 2)

-- Sanity checks:
-- (1) n = 6 is congruent via right triangle (3, 4, 5) with denominator d = 1;
-- (2) n = 5 is congruent via rational legs (20/3, 3/2, 41/6) = (40, 9, 41) over d = 6;
-- (3) (3, 4, 5) does not witness n = 1.
#guard isCongruentTriangle 3 4 5 1 6 = true
#guard isCongruentTriangle 40 9 41 6 5 = true
#guard isCongruentTriangle 3 4 5 1 1 = false
