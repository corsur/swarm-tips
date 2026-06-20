import Lproofs.Schemes.Bisect

/-! @lc 162 | name:Find Peak Element | scheme:bisection | family:binary-search | complexity:O(log n) |
    source:https://leetcode.com/problems/find-peak-element/

    Binary search for a peak: compare `nums[mid]` to `nums[mid+1]` and recurse into the higher half.
    CLASSIFICATION (bisection): each step halves the candidate index interval — `peakStep` strictly
    shrinks `hi − lo`, so the divide-and-conquer terminates in O(log n). CORRECTNESS: the reason the
    search is guaranteed to succeed is that, with the problem's distinct-adjacent guarantee, a peak
    ALWAYS exists — we certify exactly that: any global-maximum index is a strict peak (boundaries
    act as −∞), so `∃ i, isPeak nums n i`. The slope-following search can only converge onto one. -/

namespace LC.P0162
open Interview.Patterns

/-- Midpoint of the current search interval. -/
def mid (lo hi : ℕ) : ℕ := (lo + hi) / 2

/-- One bisection step: keep the half on the higher side of the midpoint slope. -/
def peakStep (f : ℕ → ℤ) (lo hi : ℕ) : ℕ × ℕ :=
  if f (mid lo hi) < f (mid lo hi + 1) then (mid lo hi + 1, hi) else (lo, mid lo hi)

/-- `i` is a peak of the length-`n` array `f`: strictly above both existing neighbours, with the
    out-of-bounds boundaries acting as `−∞`. -/
def isPeak (f : ℕ → ℤ) (n i : ℕ) : Prop :=
  i < n ∧ (i = 0 ∨ f (i - 1) < f i) ∧ (i + 1 = n ∨ f (i + 1) < f i)

/-- SCHEME (bisection): each step strictly halves the candidate interval `hi − lo`, so the
    divide-and-conquer is well-founded and runs in O(log n). -/
theorem cls (f : ℕ → ℤ) (lo hi : ℕ) (h : lo < hi) :
    (peakStep f lo hi).2 - (peakStep f lo hi).1 < hi - lo := by
  unfold peakStep mid
  split <;> omega

/-- CORRECT: with distinct adjacent values, a peak always exists — any global maximum is one.
    This is the invariant guaranteeing the slope-following binary search converges to a peak. -/
theorem corr (f : ℕ → ℤ) (n : ℕ) (hn : 0 < n)
    (hadj : ∀ i, i + 1 < n → f i ≠ f (i + 1)) :
    ∃ i, isPeak f n i := by
  obtain ⟨i, hi_mem, hi_max⟩ :=
    Finset.exists_max_image (Finset.range n) f ⟨0, Finset.mem_range.mpr hn⟩
  have hin : i < n := Finset.mem_range.mp hi_mem
  refine ⟨i, hin, ?_, ?_⟩
  · rcases Nat.eq_zero_or_pos i with h0 | hpos
    · exact Or.inl h0
    · refine Or.inr ?_
      have hlt : i - 1 < n := by omega
      have hle := hi_max (i - 1) (Finset.mem_range.mpr hlt)
      have hne := hadj (i - 1) (by omega)
      have hk : i - 1 + 1 = i := by omega
      rw [hk] at hne
      exact lt_of_le_of_ne hle hne
  · rcases Nat.lt_or_ge (i + 1) n with hlt | hge
    · refine Or.inr ?_
      have hle := hi_max (i + 1) (Finset.mem_range.mpr hlt)
      have hne := (hadj i hlt).symm
      exact lt_of_le_of_ne hle hne
    · exact Or.inl (by omega)

end LC.P0162
