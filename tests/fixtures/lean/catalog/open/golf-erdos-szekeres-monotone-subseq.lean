-- swarm:max_proof_lines=65
-- swarm:max_proof_bytes=3600
-- swarm:max_line_width=120
import Mathlib.Order.Fin.Basic
import Mathlib.Data.Real.Basic
import Mathlib.Order.Monotone.Basic

/-!
# Proof-Compression Bounty: Finite Erdős–Szekeres Theorem (≤ 65 Lines, ≤ 3600 Bytes)

The finite Erdős–Szekeres theorem (1935) states that every sequence of `r * s + 1` real
numbers contains either a non-decreasing subsequence of length `r + 1` or a non-increasing
subsequence of length `s + 1`. The bound `r * s + 1` is sharp for all `r, s : ℕ`.

While Mathlib includes the infinitary version on `ℕ`, the sharp finite pigeonhole theorem
(e.g., Seidenberg's 1959 pair-labeling proof) is not a one-liner. This compression bounty
rewards an idiomatic Lean 4 proof within `≤ 65` non-comment code lines, `≤ 3600` code bytes,
and `≤ 120` characters per line.
-/

/-- Finite Erdős–Szekeres Theorem: every sequence `f : Fin (r * s + 1) → ℝ` admits either
    a strictly increasing index map `g : Fin (r + 1) → Fin (r * s + 1)` with `Monotone (f ∘ g)`
    or a strictly increasing index map `h : Fin (s + 1) → Fin (r * s + 1)` with `Antitone (f ∘ h)`. -/
def statementProp : Prop :=
  ∀ (r s : ℕ) (f : Fin (r * s + 1) → ℝ),
    (∃ g : Fin (r + 1) → Fin (r * s + 1), StrictMono g ∧ Monotone (f ∘ g)) ∨
    (∃ h : Fin (s + 1) → Fin (r * s + 1), StrictMono h ∧ Antitone (f ∘ h))
