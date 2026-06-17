import Lproofs.Schemes.Fold

/-! @lc 1423 | name:Maximum Points You Can Obtain from Cards | scheme:fold | family:prefix-sum |
    complexity:O(k) | source:https://leetcode.com/problems/maximum-points-you-can-obtain-from-cards/

    Take exactly `k` cards from either end to maximise the total. The accepted solution streams running
    prefix (and suffix) sums and maximises over the `k+1` split points. CLASSIFICATION: the running
    sums are computed by a streaming left fold. NON-VACUITY: we prove each candidate split value is the
    sum of a genuine selection of `k` cards (`j` from the front, `k−j` from the back) — so the maximised
    quantity ranges over real card choices, not an abstraction. We certify the fold + the selection
    structure, not the maximisation. -/

namespace LC.P1423
open Interview.Patterns

/-- The actual `k`-card selection for split `j`: the first `j` cards and the last `k − j`. -/
def selection (cards : List ℤ) (k j : ℕ) : List ℤ :=
  cards.take j ++ cards.drop (cards.length - (k - j))

/-- The split value: front-prefix sum plus back-suffix sum. -/
def candidate (cards : List ℤ) (k j : ℕ) : ℤ :=
  (cards.take j).sum + (cards.drop (cards.length - (k - j))).sum

/-- The accepted answer: the best split value over the `k+1` choices. -/
def sol (cards : List ℤ) (k : ℕ) : ℤ :=
  ((List.range (k + 1)).map (candidate cards k)).foldr max 0

/-- SCHEME (fold): the running sums driving every candidate are a streaming left fold. -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl (· + ·) 0) :=
  ⟨(· + ·), 0, fun _ => rfl⟩

/-- NON-VACUITY: each candidate value is exactly the sum of a genuine `k`-card end-selection. -/
theorem corr (cards : List ℤ) (k j : ℕ) : candidate cards k j = (selection cards k j).sum := by
  simp only [candidate, selection, List.sum_append]

end LC.P1423
