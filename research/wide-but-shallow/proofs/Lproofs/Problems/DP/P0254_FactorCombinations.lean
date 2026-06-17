import Lproofs.Schemes.Fold

/-! @lc 254 | name:Factor Combinations | scheme:dp | family:enumeration | complexity:exp |
    source:https://leetcode.com/problems/factor-combinations/

    Enumerate the ways to write `n` as a product of factors ≥ 2. CLASSIFICATION: a recursive
    decomposition — pick a divisor `d` (≥ the previous factor, a proper divisor), emit it, and recurse on
    `n / d` (`cls`). NON-VACUITY: we prove soundness — every enumerated factorization multiplies back to
    `n` (`corr`) — so the decomposition explores real factorizations, not a re-encoding. We certify the
    decomposition + soundness. -/

namespace LC.P0254

/-- Factorizations of `n` into factors ≥ `start` (fuel bounds the recursion). -/
def facts : ℕ → ℕ → ℕ → List (List ℕ)
  | 0, _, _ => []
  | f + 1, n, start =>
      (List.range (n + 1)).flatMap (fun d =>
        if start ≤ d ∧ 1 < d ∧ d < n ∧ d ∣ n then (facts f (n / d) d).map (d :: ·) else []) ++
      (if 1 < n then [[n]] else [])

/-- SCHEME (dp / recursive decomposition): pick a proper divisor `d ≥ start`, emit it, recurse on
    `n / d`; or take `n` itself as a single factor. -/
theorem cls (f n start : ℕ) :
    facts (f + 1) n start =
      (List.range (n + 1)).flatMap (fun d =>
        if start ≤ d ∧ 1 < d ∧ d < n ∧ d ∣ n then (facts f (n / d) d).map (d :: ·) else []) ++
      (if 1 < n then [[n]] else []) := rfl

/-- NON-VACUITY (soundness): every enumerated factorization multiplies back to `n`. -/
theorem corr : ∀ (f n start : ℕ) (fac : List ℕ), fac ∈ facts f n start → fac.prod = n := by
  intro f
  induction f with
  | zero => intro n start fac h; simp [facts] at h
  | succ f ih =>
    intro n start fac h
    rw [cls, List.mem_append] at h
    rcases h with h | h
    · rw [List.mem_flatMap] at h
      obtain ⟨d, _, hd⟩ := h
      split at hd
      · rename_i hcond
        rw [List.mem_map] at hd
        obtain ⟨rest, hrest, rfl⟩ := hd
        have hprod := ih (n / d) d rest hrest
        rw [List.prod_cons, hprod, Nat.mul_div_cancel' hcond.2.2.2]
      · simp at hd
    · split at h
      · rw [List.mem_singleton] at h; subst h; simp
      · simp at h

end LC.P0254
