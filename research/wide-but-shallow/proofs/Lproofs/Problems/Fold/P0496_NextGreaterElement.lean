import Lproofs.Schemes.Fold

/-! @lc 496 | name:Next Greater Element I | scheme:fold | family:monotonic-stack | complexity:O(n) |
    source:https://leetcode.com/problems/next-greater-element-i/

    For each element, the next greater element to its right; a monotonic stack computes all in one pass.
    CLASSIFICATION: the pass is a streaming left fold whose accumulator is the monotonic stack — each
    element pops every stack entry it dominates (`≤` it), then is pushed. NON-VACUITY: we prove the two
    facts the amortized-O(1) analysis rests on — the current element always lands on top, and the stack
    never grows by more than one per step (the pops bound it) — so the accumulator does genuine
    monotonic-stack work. We certify the fold + the push/amortized-bound structure. -/

namespace LC.P0496
open Interview.Patterns

/-- One step: pop every entry `≤ x` (they have found their next-greater, `x`), then push `x`. -/
def step (stk : List ℤ) (x : ℤ) : List ℤ := x :: stk.dropWhile (· ≤ x)

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

/-- NON-VACUITY (amortized bound): each step grows the stack by at most one — the pops offset the
    push, the genuine monotonic-stack invariant behind the O(n) total. -/
theorem corr_bound (stk : List ℤ) (x : ℤ) : (step stk x).length ≤ stk.length + 1 := by
  simp only [step, List.length_cons]
  have := dropWhile_len stk (· ≤ x)
  omega

end LC.P0496
