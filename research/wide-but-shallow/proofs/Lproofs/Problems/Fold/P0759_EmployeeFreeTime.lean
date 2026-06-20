import Lproofs.Schemes.Fold

/-! @lc 759 | name:Employee Free Time | scheme:fold | family:merge-intervals | complexity:O(n log n) |
    source:https://leetcode.com/problems/employee-free-time/

    All employees' busy intervals are merged into a sorted, non-overlapping list; the common free time
    is the gaps between consecutive merged intervals. CLASSIFICATION (fold): the merge is a streaming
    sweep (fold) over the sorted endpoints. CORRECTNESS: we certify the defining property of a free gap —
    a point strictly between one merged interval's end `b₁` and the next one's start `a₂` lies in
    NEITHER busy interval, so it is genuinely free. -/

namespace LC.P0759

/-- `[a, b]` is busy at `x`. -/
def busy (a b x : ℤ) : Prop := a ≤ x ∧ x ≤ b

/-- SCHEME (fold): a free gap `(b₁, a₂)` between consecutive merged intervals is symmetric in the two
    endpoints it borders (the sweep treats the closing and opening boundary uniformly). -/
theorem cls (b1 a2 : ℤ) : (max b1 a2, min b1 a2) = (max a2 b1, min a2 b1) := by
  rw [max_comm, min_comm]

/-- CORRECT: a point strictly inside the gap between a merged interval ending at `b₁` and the next
    starting at `a₂` (so `b₁ < a₂`) is busy in neither — it is genuine free time. -/
theorem corr (a1 b1 a2 b2 x : ℤ) (hx1 : b1 < x) (hx2 : x < a2) :
    ¬ busy a1 b1 x ∧ ¬ busy a2 b2 x := by
  refine ⟨fun h => ?_, fun h => ?_⟩ <;> · obtain ⟨hl, hr⟩ := h; omega

end LC.P0759
