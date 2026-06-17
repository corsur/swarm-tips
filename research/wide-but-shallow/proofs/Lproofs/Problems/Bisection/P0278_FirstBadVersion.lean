import Lproofs.Schemes.Bisect

/-! @lc 278 | name:First Bad Version | scheme:bisection | family:binary-search | complexity:O(log n) |
    source:https://leetcode.com/problems/first-bad-version/ -/

namespace LC.P0278
open Interview.Patterns

/-- Editorial binary search: the first bad version is the decision boundary `Nat.find`.
    (`bad v` = version `v` is bad; monotone — once bad, all later versions are bad.) -/
def sol (bad : ℕ → Prop) [DecidablePred bad] (h : ∃ n, bad n) : ℕ := Nat.find h

/-- Spec: the answer is the least bad version. -/
def spec (bad : ℕ → Prop) (n : ℕ) : Prop := IsLeast {m | bad m} n

/-- SCHEME (bisection): the monotone predicate is a threshold — `bad n ↔ answer ≤ n` — which is
    exactly the up-set structure that makes halving on the boundary correct. -/
theorem cls (bad : ℕ → Prop) [DecidablePred bad] (mono : ∀ a b, a ≤ b → bad a → bad b)
    (h : ∃ n, bad n) (n : ℕ) : bad n ↔ sol bad h ≤ n :=
  bisection_threshold bad mono h n

/-- CORRECT: the binary-search answer is the first (least) bad version. -/
theorem corr (bad : ℕ → Prop) [DecidablePred bad] (h : ∃ n, bad n) :
    spec bad (sol bad h) :=
  bisection_isLeast bad h

end LC.P0278
