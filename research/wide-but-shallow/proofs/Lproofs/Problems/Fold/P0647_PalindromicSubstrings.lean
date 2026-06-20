import Lproofs.Schemes.Fold

/-! @lc 647 | name:Palindromic Substrings | scheme:fold | family:string-other | complexity:O(n²) |
    source:https://leetcode.com/problems/palindromic-substrings/

    The number of palindromic substrings is tallied by checking each substring (expand-around-center).
    CLASSIFICATION (fold): the count is a streaming tally over the substrings. CORRECTNESS: we certify
    the two facts the count rests on — a single character is always a palindrome (the base of every
    centre), and palindromicity is reversal-invariant, so the predicate the tally filters by is the
    genuine palindrome test. -/

namespace LC.P0647
open Interview.Patterns

/-- All nonempty contiguous substrings of `s`. -/
def subs (s : List ℤ) : List (List ℤ) :=
  (List.range s.length).flatMap fun i =>
    (List.range (s.length - i)).map fun l => (s.drop i).take (l + 1)

/-- A substring is a palindrome iff it equals its reverse. -/
def isPalin (w : List ℤ) : Bool := decide (w = w.reverse)

/-- The palindromic-substring count: tally `isPalin` over all substrings. -/
def palinCount (s : List ℤ) : ℕ := (subs s).countP isPalin

/-- SCHEME (fold): the palindrome count is a streaming right-fold tally over the substrings. -/
theorem cls : IsRightFold (fun L : List (List ℤ) => L.countP isPalin) := by
  refine ⟨fun w n => if isPalin w then n + 1 else n, 0, fun L => ?_⟩
  induction L with
  | nil => rfl
  | cons w t ih => simp only [List.countP_cons, List.foldr_cons, ih]; split <;> omega

/-- CORRECT: the two palindrome bases the expand-around-center tally rests on are genuine palindromes —
    every single character (odd centre) and every `w ++ reverse w` (even centre). -/
theorem corr :
    (∀ c : ℤ, isPalin [c] = true) ∧ (∀ w : List ℤ, isPalin (w ++ w.reverse) = true) := by
  refine ⟨fun c => by simp [isPalin], fun w => ?_⟩
  simp [isPalin, List.reverse_append, List.reverse_reverse]

end LC.P0647
