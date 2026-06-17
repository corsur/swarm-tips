import Lproofs.Schemes.Bisect

/-! @lc 875 | name:Koko Eating Bananas | scheme:bisection | family:bisection-on-answer | complexity:O(n log m) |
    source:https://leetcode.com/problems/koko-eating-bananas/

    Find the least eating speed `k` at which Koko finishes all piles within `h` hours. CLASSIFICATION:
    `canFinish k` ("speed `k` finishes in time") is a monotone up-set in `k` — a faster speed never
    hurts — so the accepted binary search returns its least satisfying value (`Nat.find`). `cls`
    certifies the threshold structure (`canFinish k ↔ answer ≤ k`); `corr` that the answer is the least
    feasible speed. We certify the monotone-threshold structure the binary search exploits. -/

namespace LC.P0875
open Interview.Patterns

/-- Whether speed `k` lets Koko finish in time (the genuine feasibility test; monotone in `k`). -/
def canFinish (hours : ℕ → ℕ) (h k : ℕ) : Prop := hours k ≤ h

instance (hours : ℕ → ℕ) (h : ℕ) : DecidablePred (canFinish hours h) :=
  fun k => inferInstanceAs (Decidable (hours k ≤ h))

/-- The accepted binary-search answer: the least speed that finishes in time. -/
def sol (hours : ℕ → ℕ) (h : ℕ) (hex : ∃ k, canFinish hours h k) : ℕ := Nat.find hex

/-- Feasibility is a monotone up-set when `hours` is antitone (faster ⇒ no more time): once a speed
    finishes in time, every larger speed does too. -/
theorem canFinish_mono (hours : ℕ → ℕ) (hanti : Antitone hours) (h : ℕ) :
    ∀ a b, a ≤ b → canFinish hours h a → canFinish hours h b :=
  fun a b hab ha => le_trans (hanti hab) ha

/-- SCHEME (bisection): feasibility is a monotone threshold — `canFinish k ↔ (least feasible) ≤ k` —
    exactly what the binary search on the answer exploits. -/
theorem cls (hours : ℕ → ℕ) (hanti : Antitone hours) (h : ℕ) (hex : ∃ k, canFinish hours h k) (n : ℕ) :
    canFinish hours h n ↔ sol hours h hex ≤ n :=
  bisection_threshold (canFinish hours h) (canFinish_mono hours hanti h) hex n

/-- The answer is the least feasible speed (the bisection threshold). -/
theorem corr (hours : ℕ → ℕ) (h : ℕ) (hex : ∃ k, canFinish hours h k) :
    IsLeast {k | canFinish hours h k} (sol hours h hex) :=
  bisection_isLeast (canFinish hours h) hex

end LC.P0875
