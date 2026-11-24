-- swarm:max_proof_lines=90
-- swarm:max_proof_bytes=4500
-- swarm:max_line_width=120
import Mathlib.Data.Nat.Prime.Basic
import Mathlib.Algebra.BigOperators.Group.Finset.Basic

/-!
# Proof-Compression Bounty: Sylvester–Schur Theorem (≤ 90 Lines, ≤ 4500 Bytes)

The Sylvester–Schur theorem (Sylvester 1892, Schur 1929, Erdős 1934) generalizes Bertrand's
postulate to arbitrary blocks of consecutive integers: for every `1 ≤ k < n`, the product
`(n + 1)(n + 2) ⋯ (n + k)` is divisible by a prime `p > k`.

In the post-RH era of multi-million-line AI proofs, this compression bounty challenges
provers to distill Erdős's 1934 elementary binomial-valuation argument into a clean,
readable Lean 4 proof within `≤ 90` non-comment code lines, `≤ 4500` code bytes, and
`≤ 120` characters per line.
-/

/-- Sylvester–Schur Theorem: for all integers `n` and `k` with `1 ≤ k` and `k < n`,
    the product `∏_{i=0}^{k-1} (n + 1 + i)` has a prime divisor strictly larger than `k`. -/
def statementProp : Prop :=
  ∀ (n k : ℕ), 1 ≤ k → k < n →
    ∃ (p : ℕ), Nat.Prime p ∧ k < p ∧
      p ∣ Finset.prod (Finset.range k) (fun i => n + 1 + i)
