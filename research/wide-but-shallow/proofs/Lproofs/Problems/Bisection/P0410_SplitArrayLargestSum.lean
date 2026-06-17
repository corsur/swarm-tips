import Lproofs.Schemes.Bisect

/-! @lc 410 | name:Split Array Largest Sum | scheme:bisection | family:bisection-on-answer |
    complexity:O(n log S) | source:https://leetcode.com/problems/split-array-largest-sum/

    Minimize, over all ways to split the array into `m` contiguous parts, the largest part sum.
    CLASSIFICATION: `canSplit cap` ("the array splits into ≤ m parts each summing to ≤ cap") is a
    monotone up-set in `cap` — a larger cap only admits more splits — so the accepted binary search on
    the answer returns its least satisfying value (`Nat.find`). `cls` certifies the threshold structure
    (`canSplit cap ↔ answer ≤ cap`); `corr` that the answer is the least feasible cap. We certify the
    monotone-threshold structure the binary search exploits. -/

namespace LC.P0410
open Interview.Patterns

/-- Whether the array splits into ≤ m parts each ≤ `cap` (the genuine feasibility test; the minimum
    number of parts needed, `minParts cap`, is antitone in `cap`). -/
def canSplit (minParts : ℕ → ℕ) (m cap : ℕ) : Prop := minParts cap ≤ m

instance (minParts : ℕ → ℕ) (m : ℕ) : DecidablePred (canSplit minParts m) :=
  fun cap => inferInstanceAs (Decidable (minParts cap ≤ m))

/-- The accepted binary-search answer: the least feasible largest-part cap. -/
def sol (minParts : ℕ → ℕ) (m : ℕ) (hex : ∃ cap, canSplit minParts m cap) : ℕ := Nat.find hex

/-- Feasibility is a monotone up-set when `minParts` is antitone (a bigger cap needs no more parts):
    once a cap is feasible, every larger cap is too. -/
theorem canSplit_mono (minParts : ℕ → ℕ) (hanti : Antitone minParts) (m : ℕ) :
    ∀ a b, a ≤ b → canSplit minParts m a → canSplit minParts m b :=
  fun a b hab ha => le_trans (hanti hab) ha

/-- SCHEME (bisection): feasibility is a monotone threshold — `canSplit cap ↔ (least feasible) ≤ cap`
    — exactly what the binary search on the answer exploits. -/
theorem cls (minParts : ℕ → ℕ) (hanti : Antitone minParts) (m : ℕ)
    (hex : ∃ cap, canSplit minParts m cap) (n : ℕ) :
    canSplit minParts m n ↔ sol minParts m hex ≤ n :=
  bisection_threshold (canSplit minParts m) (canSplit_mono minParts hanti m) hex n

/-- The answer is the least feasible cap (the minimized largest part sum). -/
theorem corr (minParts : ℕ → ℕ) (m : ℕ) (hex : ∃ cap, canSplit minParts m cap) :
    IsLeast {cap | canSplit minParts m cap} (sol minParts m hex) :=
  bisection_isLeast (canSplit minParts m) hex

end LC.P0410
