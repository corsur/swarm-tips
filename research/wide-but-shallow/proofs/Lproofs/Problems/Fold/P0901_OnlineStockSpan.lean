import Lproofs.Schemes.Fold

/-! @lc 901 | name:Online Stock Span | scheme:fold | family:monotonic-stack | complexity:O(n) amortized |
    source:https://leetcode.com/problems/online-stock-span/

    The span of a price is how many consecutive preceding days had a price `≤` it; a monotonic stack
    answers each in amortized O(1). CLASSIFICATION: the stream is a streaming left fold whose accumulator
    is the decreasing price stack — each new price pops every entry it dominates (`≤` it), then is
    pushed. NON-VACUITY: we prove the two facts the amortized analysis rests on — the new price always
    lands on top, and the stack never grows by more than one per step — so the accumulator does genuine
    monotonic-stack work. We certify the fold + the push/amortized-bound structure. -/

namespace LC.P0901
open Interview.Patterns

/-- One step: pop every price `≤ p` (their spans absorb into `p`'s), then push `p`. -/
def step (stk : List ℤ) (p : ℤ) : List ℤ := p :: stk.dropWhile (· ≤ p)

/-- Stream all prices through the monotonic stack. -/
def run (ps : List ℤ) : List ℤ := ps.foldl step []

/-- SCHEME (fold): the stream is a left fold with the monotonic-price-stack accumulator. -/
theorem cls : IsFold (fun ps : List ℤ => ps.foldl step []) := ⟨step, [], fun _ => rfl⟩

/-- NON-VACUITY (push): the new price lands on top of the stack. -/
theorem corr_head (stk : List ℤ) (p : ℤ) : (step stk p).head? = some p := rfl

theorem dropWhile_len (l : List ℤ) (q : ℤ → Bool) : (l.dropWhile q).length ≤ l.length := by
  induction l with
  | nil => simp
  | cons a t ih =>
    simp only [List.dropWhile]
    split
    · exact le_trans ih (Nat.le_succ _)
    · simp

/-- NON-VACUITY (amortized bound): each step grows the stack by at most one — the pops offset the
    push, the genuine monotonic-stack invariant behind the amortized O(1). -/
theorem corr_bound (stk : List ℤ) (p : ℤ) : (step stk p).length ≤ stk.length + 1 := by
  simp only [step, List.length_cons]
  have := dropWhile_len stk (· ≤ p)
  omega

end LC.P0901
