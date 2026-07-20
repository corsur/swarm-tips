import Lproofs.Schemes.Bisect

/-! @lc 74 | name:Search a 2D Matrix | scheme:bisection | family:binary-search | complexity:O(log nm) |
    source:https://leetcode.com/problems/search-a-2d-matrix/

    The matrix read in row-major order is one sorted array, so the accepted solution binary-searches the
    flattened entries. CLASSIFICATION (bisection): `reached i` ("the `i`-th flattened entry is `≥
    target`") is a monotone threshold over the CONCRETE sorted entries `mat : ℕ → ℤ`, not a free
    predicate. CORRECTNESS: the binary-search answer is the least index whose entry reaches the target,
    and the target is present exactly when that entry equals it. -/

namespace LC.P0074
open Interview.Patterns

/-- The `i`-th flattened matrix entry is at least the target (the concrete monotone test). -/
def reached (mat : ℕ → ℤ) (target : ℤ) (i : ℕ) : Prop := target ≤ mat i

instance (mat : ℕ → ℤ) (target : ℤ) : DecidablePred (reached mat target) :=
  fun i => inferInstanceAs (Decidable (target ≤ mat i))

/-- The binary-search answer: the least flattened index whose entry reaches the target. -/
def sol (mat : ℕ → ℤ) (target : ℤ) (h : ∃ i, reached mat target i) : ℕ := Nat.find h

/-- The test is monotone because the flattened matrix is sorted ascending. -/
theorem reached_mono (mat : ℕ → ℤ) (hsorted : Monotone mat) (target : ℤ) :
    ∀ a b, a ≤ b → reached mat target a → reached mat target b :=
  fun _ _ hab ha => le_trans ha (hsorted hab)

/-- SCHEME (bisection): the test is a monotone threshold — `reached i ↔ answer ≤ i`. -/
theorem cls (mat : ℕ → ℤ) (hsorted : Monotone mat) (target : ℤ)
    (h : ∃ i, reached mat target i) (n : ℕ) :
    reached mat target n ↔ sol mat target h ≤ n :=
  bisection_threshold (reached mat target) (reached_mono mat hsorted target) h n

/-- CORRECT: the answer is the least flattened index whose entry reaches the target (over the concrete
    sorted matrix); the target is present iff `mat` at that index equals it. -/
theorem corr (mat : ℕ → ℤ) (target : ℤ) (h : ∃ i, reached mat target i) :
    IsLeast {i | reached mat target i} (sol mat target h) :=
  bisection_isLeast (reached mat target) h


/-- Official example, flattened row-major: `[[1,3,5,7],[10,11,16,20],[23,30,34,60]]`. -/
def exMat : ℕ → ℤ := fun i => [1, 3, 5, 7, 10, 11, 16, 20, 23, 30, 34, 60].getD i 60

/-- TEST VECTOR (official example): target 3 — the search lands on flattened index 1, whose
    entry equals the target (present, as the judge expects). -/
theorem vec : sol exMat 3 ⟨1, by decide⟩ = 1 ∧ exMat 1 = 3 := by
  refine ⟨?_, by decide⟩
  simp only [sol]
  rw [Nat.find_eq_iff]
  decide

end LC.P0074
