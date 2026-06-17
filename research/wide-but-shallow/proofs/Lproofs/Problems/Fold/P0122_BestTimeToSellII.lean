import Lproofs.Schemes.Fold

/-! @lc 122 | name:Best Time to Buy and Sell Stock II | scheme:fold | family:greedy | complexity:O(n) |
    source:https://leetcode.com/problems/best-time-to-buy-and-sell-stock-ii/

    With unlimited transactions the maximum profit is the sum of every positive day-to-day price rise.
    The accepted one-pass solution streams that sum. CLASSIFICATION: a left fold carrying a bounded
    `(previous price, profit)` accumulator, adding `max(0, today − yesterday)` each step. NON-VACUITY:
    we prove the fold computes exactly the sum of positive adjacent differences (`diffSum`), pinning the
    step to real incremental work. We certify the fold + that quantity, not its optimality. -/

namespace LC.P0122
open Interview.Patterns

/-- Sum of positive day-to-day rises. -/
def diffSum : List ℤ → ℤ
  | [] => 0
  | [_] => 0
  | a :: b :: rest => max 0 (b - a) + diffSum (b :: rest)

/-- One pass step: carry the previous price; add the positive rise. -/
def step (st : Option ℤ × ℤ) (p : ℤ) : Option ℤ × ℤ :=
  match st.1 with
  | some q => (some p, st.2 + max 0 (p - q))
  | none => (some p, st.2)

/-- Stream the prices into `(last price, profit)`. -/
def scan (prices : List ℤ) : Option ℤ × ℤ := prices.foldl step (none, 0)

/-- SCHEME (fold): the pass is a left fold with a bounded `(prev, profit)` accumulator. -/
theorem cls : IsFold (fun prices : List ℤ => prices.foldl step (none, 0)) :=
  ⟨step, (none, 0), fun _ => rfl⟩

/-- Folding from `(some q, acc)` adds the positive rises of `q :: prices`. -/
theorem foldl_step (prices : List ℤ) (q acc : ℤ) :
    (prices.foldl step (some q, acc)).2 = acc + diffSum (q :: prices) := by
  induction prices generalizing q acc with
  | nil => simp [diffSum]
  | cons p ps ih =>
    rw [List.foldl_cons]
    show (ps.foldl step (some p, acc + max 0 (p - q))).2 = _
    rw [ih p (acc + max 0 (p - q))]
    simp only [diffSum]
    ring

/-- NON-VACUITY: the fold computes exactly the sum of positive adjacent rises. -/
theorem corr (prices : List ℤ) : (scan prices).2 = diffSum prices := by
  cases prices with
  | nil => simp [scan, diffSum]
  | cons p ps =>
    rw [scan, List.foldl_cons]
    show (ps.foldl step (some p, 0)).2 = _
    rw [foldl_step ps p 0]
    simp [diffSum]

end LC.P0122
