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

/-- The per-value tally the Boyer–Moore verification pass computes. -/
def sol (a : ℤ) (L : List ℤ) : ℕ := L.count a

/-- SCHEME (fold): the per-value tally `sol` is a streaming right fold over the array. -/
theorem cls (a : ℤ) : IsRightFold (sol a) := by
  refine ⟨fun x n => if x == a then n + 1 else n, 0, fun L => ?_⟩
  induction L with
  | nil => rfl
  | cons x t ih =>
    simp only [sol, List.count_cons, List.foldr_cons] at *
    rw [← ih]
    split <;> omega

/-- CORRECT: at most two values can each occur more than ⌊n/3⌋ times — three pairwise-distinct values
    each exceeding `n/3` is impossible, since their disjoint occurrences would exceed the array. -/
theorem corr (nums : List ℤ) (a b c : ℤ) (hab : a ≠ b) (hbc : b ≠ c) (hac : a ≠ c)
    (ha : 3 * sol a nums > nums.length) (hb : 3 * sol b nums > nums.length)
    (hc : 3 * sol c nums > nums.length) : False := by
  have := count3_le a b c hab hbc hac nums
  simp only [sol] at ha hb hc
  omega


/-- GROUND INSTANCE (official example 1): in [3,2,3] the value 3 tallies 2 > ⌊3/3⌋ (a majority
    element) and 2 tallies 1 (not). -/
theorem vec : sol 3 [3, 2, 3] = 2 ∧ sol 2 [3, 2, 3] = 1 := by decide

end LC.P0229
