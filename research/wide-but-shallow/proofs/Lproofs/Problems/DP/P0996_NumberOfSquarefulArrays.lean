import Lproofs.Schemes.Fold

/-! @lc 996 | name:Number of Squareful Arrays | scheme:dp | family:backtracking | complexity:O(n!·n) |
    source:https://leetcode.com/problems/number-of-squareful-arrays/

    Count the permutations of `nums` in which every adjacent pair sums to a perfect square. The accepted
    solution is backtracking that extends a partial arrangement only when the new adjacent sum is a
    perfect square (with duplicate pruning). CLASSIFICATION (dp / backtracking): the count tallies the
    arrangements passing the adjacency check, a fold. CORRECTNESS (the pruning predicate, not the final
    count): we certify that the adjacency check the search prunes on is sound and complete---an
    arrangement passes it exactly when every adjacent sum really is a perfect square. -/

namespace LC.P0996
open Interview.Patterns

/-- A perfect-square test (decidable via integer sqrt). -/
def isSq (n : ℕ) : Bool := Nat.sqrt n * Nat.sqrt n == n

/-- The pruning check the backtracking uses: every adjacent pair sums to a perfect square. -/
def adjSq : List ℕ → Bool
  | x :: y :: t => isSq (x + y) && adjSq (y :: t)
  | _ => true

/-- The specification: every adjacent pair sums to a perfect square. -/
def adjSqSpec : List ℕ → Prop
  | x :: y :: t => isSq (x + y) = true ∧ adjSqSpec (y :: t)
  | _ => True

/-- SCHEME (fold): the squareful count is a streaming tally over the candidate arrangements. -/
theorem cls : IsRightFold (fun L : List (List ℕ) => L.countP adjSq) := by
  refine ⟨fun a n => if adjSq a then n + 1 else n, 0, fun L => ?_⟩
  induction L with
  | nil => rfl
  | cons a t ih => simp only [List.countP_cons, List.foldr_cons, ih]; split <;> omega

/-- CORRECT: the adjacency check is sound and complete---an arrangement passes exactly when every
    adjacent sum is a perfect square. So the search prunes on precisely the squareful condition. -/
theorem corr (arr : List ℕ) : adjSq arr = true ↔ adjSqSpec arr := by
  induction arr with
  | nil => simp [adjSq, adjSqSpec]
  | cons x t ih =>
    cases t with
    | nil => simp [adjSq, adjSqSpec]
    | cons y t' =>
      simp only [adjSq, adjSqSpec, Bool.and_eq_true]
      rw [ih]

end LC.P0996
