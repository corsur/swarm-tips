import Lproofs.Schemes.Fold

/-! @lc 1385 | name:Find the Distance Value Between Two Arrays | scheme:fold | family:binary-search |
    complexity:O(n log m) | source:https://leetcode.com/problems/find-the-distance-value-between-two-arrays/

    The distance value counts the elements of `arr1` for which NO element of `arr2` lies within `d`.
    CLASSIFICATION (fold): the count is a streaming tally over `arr1`. CORRECTNESS: we certify the
    genuine monotonicity the binary-search variant exploits — the distance value is antitone in the
    threshold `d` (a larger tolerance can only leave fewer far-from-everything elements), proven over the
    concrete arrays. -/

namespace LC.P1385
open Interview.Patterns

/-- `x` is farther than `d` from every one of the first `m2` entries of `arr2`. -/
def farFromAll (arr2 : ℕ → ℕ) (m2 x d : ℕ) : Bool :=
  decide (∀ j < m2, d < max x (arr2 j) - min x (arr2 j))

/-- The distance value: how many of the first `m1` entries of `arr1` are far from all of `arr2`. -/
def sol (arr1 arr2 : ℕ → ℕ) (m1 m2 d : ℕ) : ℕ :=
  (List.range m1).countP fun i => farFromAll arr2 m2 (arr1 i) d

/-- SCHEME (fold): the distance value is a streaming right-fold tally over the indices of
    `arr1` — and `sol` is exactly that tally over `range m1`. -/
theorem cls (arr1 arr2 : ℕ → ℕ) (m2 d : ℕ) :
    (IsRightFold (fun L : List ℕ => L.countP fun i => farFromAll arr2 m2 (arr1 i) d)) ∧
    ∀ m1, sol arr1 arr2 m1 m2 d =
      (List.range m1).countP fun i => farFromAll arr2 m2 (arr1 i) d := by
  refine ⟨⟨fun i n => if farFromAll arr2 m2 (arr1 i) d then n + 1 else n, 0, fun L => ?_⟩,
    fun _ => rfl⟩
  induction L with
  | nil => rfl
  | cons x t ih => simp only [List.countP_cons, List.foldr_cons, ih]; split <;> omega

/-- CORRECT: the distance value is antitone in the threshold `d` — widening the tolerance can only
    reduce how many elements stay far from all of `arr2`. Proven over the concrete arrays. -/
theorem corr (arr1 arr2 : ℕ → ℕ) (m1 m2 : ℕ) : Antitone (sol arr1 arr2 m1 m2) := by
  intro a b hab
  apply List.countP_mono_left
  intro i _ hi
  simp only [farFromAll, decide_eq_true_eq] at hi ⊢
  exact fun j hj => lt_of_le_of_lt hab (hi j hj)


/-- GROUND INSTANCE (official example 1): arr1 = [4,5,8], arr2 = [10,9,1,8], d = 2 — distance
    value 2 (4 and 5 qualify, 8 does not). -/
theorem vec : sol (fun i => [4, 5, 8].getD i 0) (fun j => [10, 9, 1, 8].getD j 0) 3 4 2 = 2 := by
  decide

end LC.P1385
