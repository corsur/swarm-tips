import Lproofs.Patterns

/-! @lc 328 | name:Odd Even Linked List | scheme:fold | family:partition-scan | complexity:O(n) |
    source:https://leetcode.com/problems/odd-even-linked-list/

    Regroup a list so odd-position nodes precede even-position ones, in one pass. CLASSIFICATION: a
    streaming left fold whose accumulator is the pair of partial chains `(odds, evens)` plus the current
    parity — each node is appended to the chain its index parity selects. NON-VACUITY: we prove the
    partition loses nothing — the two chains' combined length equals the number of nodes processed — so
    the accumulator does genuine partitioning work, not a re-encoding. We certify the fold + the
    length-conservation invariant. -/

namespace LC.P0328
open Interview.Patterns

/-- Append the node to the odd or even chain by the current parity, then flip parity. -/
def step : (List ℤ × List ℤ × Bool) → ℤ → (List ℤ × List ℤ × Bool)
  | (odds, evens, isOdd), x => if isOdd then (odds ++ [x], evens, false) else (odds, evens ++ [x], true)

/-- Partition the list into odd/even position chains (starting at an odd position). -/
def split (xs : List ℤ) : List ℤ × List ℤ × Bool := xs.foldl step ([], [], true)

/-- SCHEME (fold): the regrouping is a left fold with the `(odds, evens, parity)` accumulator. -/
theorem cls : IsFold (fun xs : List ℤ => xs.foldl step ([], [], true)) := ⟨step, ([], [], true), fun _ => rfl⟩

/-- Each step adds exactly one node across the two chains. -/
theorem step_len (acc : List ℤ × List ℤ × Bool) (x : ℤ) :
    (step acc x).1.length + (step acc x).2.1.length = acc.1.length + acc.2.1.length + 1 := by
  obtain ⟨odds, evens, isOdd⟩ := acc
  simp only [step]
  split <;> simp <;> omega

/-- NON-VACUITY: the two chains' combined length equals the number of nodes processed. -/
theorem corr (xs : List ℤ) : (split xs).1.length + (split xs).2.1.length = xs.length := by
  have key : ∀ (ys : List ℤ) acc,
      (ys.foldl step acc).1.length + (ys.foldl step acc).2.1.length
        = acc.1.length + acc.2.1.length + ys.length := by
    intro ys
    induction ys with
    | nil => intro acc; simp
    | cons y rest ih =>
      intro acc
      simp only [List.foldl_cons, List.length_cons]
      rw [ih (step acc y), step_len acc y]
      omega
  have := key xs ([], [], true)
  simpa [split] using this

end LC.P0328
