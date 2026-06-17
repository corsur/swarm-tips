import Lproofs.Schemes.Bisect

/-! @lc 35 | name:Search Insert Position | scheme:bisection | family:binary-search | complexity:O(log n) |
    source:https://leetcode.com/problems/search-insert-position/

    In a sorted array, the insert position of `target` is the least index whose value is `≥ target`.
    Modelling the sorted array as a monotone sequence `nums`, the predicate `target ≤ nums i` is a
    monotone threshold, and the accepted binary search returns its least satisfying index.
    CLASSIFICATION ONLY: we certify the threshold structure (the predicate is a monotone up-set, so
    the answer is its `Nat.find`). -/

namespace LC.P0035
open Interview.Patterns

/-- `atLeast i` = the `i`-th value reaches `target` (the sorted array is the monotone `nums`). -/
def atLeast (nums : ℕ → ℕ) (target i : ℕ) : Prop := target ≤ nums i

instance (nums : ℕ → ℕ) (target : ℕ) : DecidablePred (atLeast nums target) :=
  fun i => inferInstanceAs (Decidable (target ≤ nums i))

/-- The accepted binary-search answer: the least index whose value is `≥ target`. -/
def sol (nums : ℕ → ℕ) (target : ℕ) (h : ∃ i, atLeast nums target i) : ℕ := Nat.find h

/-- `atLeast` is a monotone up-set when `nums` is sorted (monotone): once reached, it stays. -/
theorem atLeast_mono (nums : ℕ → ℕ) (hmono : Monotone nums) (target : ℕ) :
    ∀ a b, a ≤ b → atLeast nums target a → atLeast nums target b :=
  fun a b hab ha => le_trans ha (hmono hab)

/-- SCHEME (bisection): the predicate is a monotone threshold —
    `atLeast n ↔ (least reaching index) ≤ n` — exactly what the binary search exploits. -/
theorem cls (nums : ℕ → ℕ) (hmono : Monotone nums) (target : ℕ) (h : ∃ i, atLeast nums target i) (n : ℕ) :
    atLeast nums target n ↔ sol nums target h ≤ n :=
  bisection_threshold (atLeast nums target) (atLeast_mono nums hmono target) h n

/-- The answer is the least reaching index (the insert position / threshold). -/
theorem corr (nums : ℕ → ℕ) (target : ℕ) (h : ∃ i, atLeast nums target i) :
    IsLeast {i | atLeast nums target i} (sol nums target h) :=
  bisection_isLeast (atLeast nums target) h

end LC.P0035
