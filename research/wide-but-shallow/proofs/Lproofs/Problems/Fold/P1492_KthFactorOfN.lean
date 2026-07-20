import Lproofs.Schemes.Fold

/-! @lc 1492 | name:The kth Factor of n | scheme:fold | family:math-bit | complexity:O(n) |
    source:https://leetcode.com/problems/the-k-th-factor-of-n/

    Scan `1..n`, collecting the divisors of `n` in order; the answer is the k-th one. CLASSIFICATION
    (fold): the divisor count is a streaming tally over the candidates. CORRECTNESS: we certify that the
    collected list contains exactly the genuine divisors — every value the scan keeps is a positive
    divisor of `n` (soundness), so the k-th element really is the k-th factor. -/

namespace LC.P1492
open Interview.Patterns

/-- The divisors of `n`, in increasing order, as the scan collects them. -/
def sol (n : ℕ) : List ℕ := (List.range (n + 1)).filter fun d => decide (0 < d ∧ d ∣ n)

/-- SCHEME (fold): the divisor scan is a streaming right-fold over the candidates — and `sol`
    keeps exactly the candidates that scan accepts. -/
theorem cls (n : ℕ) :
    (IsRightFold (fun L : List ℕ => L.countP fun d => decide (0 < d ∧ d ∣ n))) ∧
    sol n = (List.range (n + 1)).filter (fun d => decide (0 < d ∧ d ∣ n)) := by
  refine ⟨⟨fun d c => if decide (0 < d ∧ d ∣ n) then c + 1 else c, 0, fun L => ?_⟩, rfl⟩
  induction L with
  | nil => rfl
  | cons d t ih => simp only [List.countP_cons, List.foldr_cons, ih]; split <;> omega

/-- CORRECT: every value the scan keeps is a genuine positive divisor of `n` — so the collected list is
    exactly the sol, and its k-th element is the k-th factor. -/
theorem corr (n d : ℕ) (h : d ∈ sol n) : 0 < d ∧ d ∣ n := by
  simp only [sol, List.mem_filter, decide_eq_true_eq] at h
  exact h.2


/-- GROUND INSTANCE (official example 1): the factors of 12 in order, whose 3rd entry (k = 3) is
    the judge's answer 3. -/
theorem vec : sol 12 = [1, 2, 3, 4, 6, 12] ∧ (sol 12).getD 2 0 = 3 := by decide


/-- COMPLETENESS (the other direction of `corr`): every positive divisor of a positive `n` is
    collected by the scan. With `corr` the list is exactly the divisors, in increasing order. -/
theorem complete (n d : ℕ) (hn : 0 < n) (h0 : 0 < d) (hd : d ∣ n) : d ∈ sol n := by
  simp only [sol, List.mem_filter, List.mem_range, decide_eq_true_eq]
  exact ⟨Nat.lt_succ_of_le (Nat.le_of_dvd hn hd), h0, hd⟩

end LC.P1492
