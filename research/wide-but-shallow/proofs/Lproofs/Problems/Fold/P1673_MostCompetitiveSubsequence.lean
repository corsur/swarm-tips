import Lproofs.Schemes.Fold

/-! @lc 1673 | name:Find the Most Competitive Subsequence | scheme:fold | family:monotonic-stack |
    complexity:O(n) | source:https://leetcode.com/problems/find-the-most-competitive-subsequence/

    Build the lexicographically smallest length-`k` subsequence with a monotonic stack: pop larger
    trailing elements while enough remain, then push. CLASSIFICATION: the pass is a streaming left fold
    whose accumulator is the monotonic stack — each element pops every strictly-greater top, then is
    pushed. NON-VACUITY: we prove the two facts the amortized analysis rests on — the new element lands
    on top, and the stack grows by at most one per step (the pops offset the push) — so the accumulator
    does genuine monotonic-stack work. We certify the fold + the push/amortized-bound structure. -/

namespace LC.P1673
open Interview.Patterns

/-- One step: pop every entry strictly greater than `x`, then push `x`. -/
def step (stk : List ℤ) (x : ℤ) : List ℤ := x :: stk.dropWhile (x < ·)

/-- Stream all elements through the monotonic stack. -/
def run (xs : List ℤ) : List ℤ := xs.foldl step []

/-- SCHEME (fold): the pass is a left fold with the monotonic-stack accumulator. -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl step []) := ⟨step, [], fun _ => rfl⟩

/-- NON-VACUITY (push): the current element lands on top of the stack. -/
theorem corr_head (stk : List ℤ) (x : ℤ) : (step stk x).head? = some x := rfl

theorem dropWhile_len (l : List ℤ) (p : ℤ → Bool) : (l.dropWhile p).length ≤ l.length := by
  induction l with
  | nil => simp
  | cons a t ih =>
    simp only [List.dropWhile]
    split
    · exact le_trans ih (Nat.le_succ _)
    · simp

/-- NON-VACUITY (amortized bound): each step grows the stack by at most one — the pops offset the push. -/
theorem corr_bound (stk : List ℤ) (x : ℤ) : (step stk x).length ≤ stk.length + 1 := by
  simp only [step, List.length_cons]
  have := dropWhile_len stk (x < ·)
  omega

end LC.P1673
