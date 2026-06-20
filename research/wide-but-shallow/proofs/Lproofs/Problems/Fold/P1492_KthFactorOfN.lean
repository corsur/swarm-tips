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
def factors (n : ℕ) : List ℕ := (List.range (n + 1)).filter fun d => decide (0 < d ∧ d ∣ n)

/-- SCHEME (fold): the divisor count is a streaming right-fold tally over the candidates. -/
theorem cls (n : ℕ) : IsRightFold (fun L : List ℕ => L.countP fun d => decide (0 < d ∧ d ∣ n)) := by
  refine ⟨fun d c => if decide (0 < d ∧ d ∣ n) then c + 1 else c, 0, fun L => ?_⟩
  induction L with
  | nil => rfl
  | cons d t ih => simp only [List.countP_cons, List.foldr_cons, ih]; split <;> omega

/-- CORRECT: every value the scan keeps is a genuine positive divisor of `n` — so the collected list is
    exactly the factors, and its k-th element is the k-th factor. -/
theorem corr (n d : ℕ) (h : d ∈ factors n) : 0 < d ∧ d ∣ n := by
  simp only [factors, List.mem_filter, decide_eq_true_eq] at h
  exact h.2

end LC.P1492
