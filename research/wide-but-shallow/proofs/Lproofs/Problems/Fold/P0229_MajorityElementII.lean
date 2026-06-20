import Lproofs.Schemes.Fold

/-! @lc 229 | name:Majority Element II | scheme:fold | family:hashing | complexity:O(n) |
    source:https://leetcode.com/problems/majority-element-ii/

    All values occurring more than ⌊n/3⌋ times are found in one Boyer–Moore pass maintaining at most
    two candidate-count pairs. CLASSIFICATION (fold): the tally is a streaming right fold over the
    array. CORRECTNESS: we certify the invariant that makes two candidates sufficient — no three
    pairwise-distinct values can each occur more than n/3 times, because their disjoint occurrences
    would exceed the whole array. -/

namespace LC.P0229
open Interview.Patterns

/-- Counts of three pairwise-distinct values are disjoint, so they sum to at most the length. -/
theorem count3_le (a b c : ℤ) (hab : a ≠ b) (hbc : b ≠ c) (hac : a ≠ c) (l : List ℤ) :
    l.count a + l.count b + l.count c ≤ l.length := by
  induction l with
  | nil => simp
  | cons x t ih =>
    simp only [List.count_cons, List.length_cons]
    have hx : (if x == a then 1 else 0) + (if x == b then 1 else 0)
                + (if x == c then 1 else 0) ≤ 1 := by
      by_cases hxa : x = a <;> by_cases hxb : x = b <;> by_cases hxc : x = c <;>
        simp_all [beq_iff_eq]
    omega

/-- SCHEME (fold): the per-value tally is a streaming right fold over the array. -/
theorem cls (a : ℤ) : IsRightFold (fun L : List ℤ => L.countP fun x => decide (x = a)) := by
  refine ⟨fun x n => if decide (x = a) then n + 1 else n, 0, fun L => ?_⟩
  induction L with
  | nil => rfl
  | cons x t ih => simp only [List.countP_cons, List.foldr_cons, ih]; split <;> omega

/-- CORRECT: at most two values can each occur more than ⌊n/3⌋ times — three pairwise-distinct values
    each exceeding `n/3` is impossible, since their disjoint occurrences would exceed the array. -/
theorem corr (nums : List ℤ) (a b c : ℤ) (hab : a ≠ b) (hbc : b ≠ c) (hac : a ≠ c)
    (ha : 3 * nums.count a > nums.length) (hb : 3 * nums.count b > nums.length)
    (hc : 3 * nums.count c > nums.length) : False := by
  have := count3_le a b c hab hbc hac nums
  omega

end LC.P0229
