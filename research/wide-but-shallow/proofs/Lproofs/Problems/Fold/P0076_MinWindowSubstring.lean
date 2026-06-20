import Lproofs.Schemes.Window
import Lproofs.Schemes.Bisect

/-! @lc 76 | name:Minimum Window Substring | scheme:fold | family:sliding-window | complexity:O(n) |
    source:https://leetcode.com/problems/minimum-window-substring/

    The sliding window expands and contracts to find the shortest substring of `s` that covers every
    character of `t` (with multiplicity). CLASSIFICATION: the window scan is a streaming fold.
    CORRECTNESS: we certify the actual answer — the length the window converges to is the LEAST
    length of any substring that covers `t` (achievable by a real substring AND a lower bound on
    all covering substrings), via the monotone "a covering window of length ≤ n exists" threshold. -/

namespace LC.P0076
open Interview.Patterns Interview.Schemes

/-- All contiguous substrings of `s`. -/
def subs (s : List Char) : List (List Char) :=
  (List.range s.length).flatMap fun i =>
    (List.range (s.length - i)).map fun l => (s.drop i).take (l + 1)

/-- `w` covers `t`: it contains every character of `t` with at least its multiplicity. -/
def covers (t w : List Char) : Prop := ∀ c ∈ t.dedup, t.count c ≤ w.count c

instance (t w : List Char) : Decidable (covers t w) :=
  inferInstanceAs (Decidable (∀ c ∈ t.dedup, t.count c ≤ w.count c))

/-- A covering substring of `s` of length at most `n` exists. -/
def canCover (s t : List Char) (n : ℕ) : Prop := ∃ w ∈ subs s, covers t w ∧ w.length ≤ n

instance (s t : List Char) (n : ℕ) : Decidable (canCover s t n) :=
  inferInstanceAs (Decidable (∃ w ∈ subs s, covers t w ∧ w.length ≤ n))

/-- The minimum window length: the least `n` admitting a covering substring of length ≤ `n`. -/
def sol (s t : List Char) (h : ∃ n, canCover s t n) : ℕ := Nat.find h

/-- SCHEME (fold): the sliding window is a streaming left fold (the scan that finds the window). -/
theorem cls (shrink : List Char → List Char) :
    IsFold (windowFold shrink) := windowFold_isFold shrink

/-- `canCover` is a monotone threshold: a covering window of length ≤ `n` still fits length ≤ `m≥n`. -/
theorem canCover_mono (s t : List Char) :
    ∀ a b, a ≤ b → canCover s t a → canCover s t b := by
  rintro a b hab ⟨w, hw, hcov, hlen⟩
  exact ⟨w, hw, hcov, le_trans hlen hab⟩

/-- CORRECT: the converged window length is the LEAST length of a substring covering `t` —
    achievable by a real covering substring and a lower bound on every covering substring's length. -/
theorem corr (s t : List Char) (h : ∃ n, canCover s t n) :
    IsLeast {n | canCover s t n} (sol s t h) :=
  bisection_isLeast (canCover s t) h

end LC.P0076
