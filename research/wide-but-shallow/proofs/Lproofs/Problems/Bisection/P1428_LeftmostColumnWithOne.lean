import Lproofs.Schemes.Bisect

/-! @lc 1428 | name:Leftmost Column with at Least a One | scheme:bisection | family:binary-search |
    complexity:O(n log m) | source:https://leetcode.com/problems/leftmost-column-with-at-least-a-one/

    Each row is sorted (zeros then ones); binary search each row for its first `1`. CLASSIFICATION: the
    predicate `hasOne j` ("column `j` is a one") is a monotone up-set along a sorted row — once a one,
    always a one — so the binary search returns its least satisfying index. `cls` certifies the threshold
    structure; `corr` that the answer is the least such column. We certify the monotone-threshold
    structure the binary search exploits. -/

namespace LC.P1428
open Interview.Patterns

/-- `hasOne j` — column `j` holds a one (a sorted row is `0…01…1`, modeled by a monotone indicator). -/
def hasOne (row : ℕ → Prop) (j : ℕ) : Prop := row j

instance (row : ℕ → Prop) [DecidablePred row] : DecidablePred (hasOne row) :=
  fun j => inferInstanceAs (Decidable (row j))

/-- The binary-search answer: the least column holding a one. -/
def sol (row : ℕ → Prop) [DecidablePred row] (h : ∃ j, hasOne row j) : ℕ := Nat.find h

/-- `hasOne` is a monotone up-set when the row is sorted: once a one, every later column is too. -/
theorem hasOne_mono (row : ℕ → Prop) (hmono : ∀ a b, a ≤ b → row a → row b) :
    ∀ a b, a ≤ b → hasOne row a → hasOne row b := hmono

/-- SCHEME (bisection): the indicator is a monotone threshold — `hasOne j ↔ leftmost ≤ j`. -/
theorem cls (row : ℕ → Prop) [DecidablePred row] (hmono : ∀ a b, a ≤ b → row a → row b)
    (h : ∃ j, hasOne row j) (n : ℕ) : hasOne row n ↔ sol row h ≤ n :=
  bisection_threshold (hasOne row) (hasOne_mono row hmono) h n

/-- The answer is the leftmost column holding a one. -/
theorem corr (row : ℕ → Prop) [DecidablePred row] (h : ∃ j, hasOne row j) :
    IsLeast {j | hasOne row j} (sol row h) :=
  bisection_isLeast (hasOne row) h

end LC.P1428
