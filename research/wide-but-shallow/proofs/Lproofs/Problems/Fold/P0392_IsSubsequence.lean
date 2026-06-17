import Lproofs.Schemes.Fold

/-! @lc 392 | name:Is Subsequence | scheme:fold | family:two-pointer | complexity:O(n) |
    source:https://leetcode.com/problems/is-subsequence/

    Decide whether `s` is a subsequence of `t`. CLASSIFICATION: the two-pointer scan is a streaming
    left fold over `t` whose accumulator is the still-unmatched suffix of `s` — each character of `t`
    advances the pointer past the head of `s` if it matches, else leaves it. NON-VACUITY: we prove the
    genuine pointer-advance recurrence — each step either keeps the remaining suffix or drops exactly
    its head — so the accumulator does real matching work; `s` is a subsequence iff the fold consumes it
    to `[]`. We certify the fold + the advance recurrence. -/

namespace LC.P0392
open Interview.Patterns

/-- Advance the unmatched suffix of `s` past `c` if it matches the head. -/
def step (rem : List Char) (c : Char) : List Char :=
  match rem with
  | r :: rs => if r = c then rs else rem
  | [] => []

/-- Consume `t`, threading the unmatched suffix of `s`. `s` is a subsequence iff the result is `[]`. -/
def consume (s t : List Char) : List Char := t.foldl step s

/-- SCHEME (fold): the match is a left fold over `t` with the unmatched-suffix accumulator. -/
theorem cls (s : List Char) : IsFold (fun t : List Char => t.foldl step s) := ⟨step, s, fun _ => rfl⟩

/-- NON-VACUITY (pointer advance): each character of `t` either leaves the remaining suffix unchanged
    or drops exactly its head — the genuine two-pointer step. -/
theorem corr (rem : List Char) (c : Char) : step rem c = rem ∨ step rem c = rem.tail := by
  match rem with
  | [] => left; rfl
  | r :: rs =>
    simp only [step, List.tail_cons]
    split
    · right; rfl
    · left; rfl

end LC.P0392
