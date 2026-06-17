import Lproofs.Schemes.Fold

/-! @lc 277 | name:Find the Celebrity | scheme:fold | family:elimination-scan | complexity:O(n) |
    source:https://leetcode.com/problems/find-the-celebrity/

    Phase one of the accepted O(n) solution sweeps once to reduce everyone to a single candidate: keep a
    running candidate, and whenever it admits knowing person `i`, switch the candidate to `i`.
    CLASSIFICATION: that sweep is a streaming left fold over the people whose accumulator is the current
    candidate. NON-VACUITY: we prove the candidate stays a valid person index throughout (it is always in
    range), so the accumulator does genuine elimination work, not a re-encoding. We certify the fold +
    the in-range invariant. -/

namespace LC.P0277
open Interview.Patterns

/-- Keep the candidate unless it knows `i`, in which case `i` becomes the candidate. -/
def step (knows : ℕ → ℕ → Bool) (cand i : ℕ) : ℕ := if knows cand i then i else cand

/-- Sweep `0 … n−1`, eliminating down to one candidate. -/
def candidate (knows : ℕ → ℕ → Bool) (n : ℕ) : ℕ := (List.range n).foldl (step knows) 0

/-- SCHEME (fold): the elimination sweep is a left fold with the running-candidate accumulator. -/
theorem cls (knows : ℕ → ℕ → Bool) : IsFold (fun ns : List ℕ => ns.foldl (step knows) 0) :=
  ⟨step knows, 0, fun _ => rfl⟩

/-- The sweep keeps the candidate in range: folding indices `< n` onto a candidate `< n` stays `< n`. -/
theorem foldl_lt (knows : ℕ → ℕ → Bool) (n : ℕ) :
    ∀ (l : List ℕ) (c : ℕ), (∀ i ∈ l, i < n) → c < n → l.foldl (step knows) c < n := by
  intro l
  induction l with
  | nil => intro c _ hc; simpa using hc
  | cons a rest ih =>
    intro c hmem hc
    simp only [List.foldl_cons]
    refine ih (step knows c a) (fun i hi => hmem i (List.mem_cons_of_mem a hi)) ?_
    have ha : a < n := hmem a List.mem_cons_self
    simp only [step]; split <;> assumption

/-- NON-VACUITY: the surviving candidate is always a valid person index. -/
theorem corr (knows : ℕ → ℕ → Bool) (n : ℕ) (hn : 0 < n) : candidate knows n < n := by
  refine foldl_lt knows n (List.range n) 0 (fun i hi => List.mem_range.mp hi) hn

end LC.P0277
