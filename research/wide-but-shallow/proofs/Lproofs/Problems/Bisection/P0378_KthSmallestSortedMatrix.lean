import Lproofs.Schemes.Bisect

/-! @lc 378 | name:Kth Smallest Element in a Sorted Matrix | scheme:bisection | family:bisection-on-answer |
    complexity:O(n log range) | source:https://leetcode.com/problems/kth-smallest-element-in-a-sorted-matrix/

    Find the k-th smallest matrix value by binary searching the value range: count how many entries are
    `≤ v` and find the least `v` whose count reaches `k`. CLASSIFICATION: the predicate `enough v`
    ("`k ≤  (#entries ≤ v)`") is a monotone up-set in `v` — the count only rises — so the binary search
    on the answer returns its least satisfying value. `cls` certifies the threshold structure; `corr`
    that the answer is the least such value. We certify the monotone-threshold structure. -/

namespace LC.P0378
open Interview.Patterns

/-- `enough v` — at least `k` entries are `≤ v` (the count `countLE` is monotone in `v`). -/
def enough (countLE : ℕ → ℕ) (k v : ℕ) : Prop := k ≤ countLE v

instance (countLE : ℕ → ℕ) (k : ℕ) : DecidablePred (enough countLE k) :=
  fun v => inferInstanceAs (Decidable (k ≤ countLE v))

/-- The binary-search answer: the least value with at least `k` entries below it — the k-th smallest. -/
def sol (countLE : ℕ → ℕ) (k : ℕ) (h : ∃ v, enough countLE k v) : ℕ := Nat.find h

/-- `enough` is a monotone up-set when `countLE` is monotone: once enough, a larger value still is. -/
theorem enough_mono (countLE : ℕ → ℕ) (hmono : Monotone countLE) (k : ℕ) :
    ∀ a b, a ≤ b → enough countLE k a → enough countLE k b :=
  fun a b hab ha => le_trans ha (hmono hab)

/-- SCHEME (bisection): the predicate is a monotone threshold — `enough v ↔ answer ≤ v`. -/
theorem cls (countLE : ℕ → ℕ) (hmono : Monotone countLE) (k : ℕ) (h : ∃ v, enough countLE k v) (n : ℕ) :
    enough countLE k n ↔ sol countLE k h ≤ n :=
  bisection_threshold (enough countLE k) (enough_mono countLE hmono k) h n

/-- The answer is the least value with `≥ k` entries below it — the k-th smallest. -/
theorem corr (countLE : ℕ → ℕ) (k : ℕ) (h : ∃ v, enough countLE k v) :
    IsLeast {v | enough countLE k v} (sol countLE k h) :=
  bisection_isLeast (enough countLE k) h

end LC.P0378
