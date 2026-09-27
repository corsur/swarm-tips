/- SPDX-License-Identifier: Apache-2.0 -/
import Mathlib
open scoped BigOperators

def statementProp : Prop :=
  ∀ (n : Nat) (G : Matrix (Fin n) (Fin n) ℂ),
    Matrix.trace (G ^ 3) =
      ∑ i : Fin n, ∑ j : Fin n, ∑ k : Fin n, G i j * G j k * G k i
