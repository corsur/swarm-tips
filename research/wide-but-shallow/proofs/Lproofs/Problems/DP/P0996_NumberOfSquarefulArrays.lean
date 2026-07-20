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

/-- The squareful tally over candidate arrangements. -/
def sol (L : List (List ℕ)) : ℕ := L.countP adjSq

/-- SCHEME (fold): the squareful count `sol` is a streaming tally over the candidates. -/
theorem cls : IsRightFold sol := by
  refine ⟨fun a n => if adjSq a then n + 1 else n, 0, fun L => ?_⟩
  unfold sol
  induction L with
  | nil => rfl
  | cons a t ih => simp only [List.countP_cons, List.foldr_cons, ih]; split <;> omega

theorem adjSq_iff (arr : List ℕ) : adjSq arr = true ↔ adjSqSpec arr := by
  induction arr with
  | nil => simp [adjSq, adjSqSpec]
  | cons x t ih =>
    cases t with
    | nil => simp [adjSq, adjSqSpec]
    | cons y t' =>
      simp only [adjSq, adjSqSpec, Bool.and_eq_true]
      rw [ih]

/-- CORRECT: `sol` counts an arrangement exactly when every adjacent sum is a perfect square —
    the search tallies and prunes on precisely the squareful condition. -/
theorem corr (arr : List ℕ) : (sol [arr] = 1 ↔ adjSqSpec arr) ∧
    (adjSq arr = true ↔ adjSqSpec arr) := by
  refine ⟨?_, adjSq_iff arr⟩
  rw [← adjSq_iff arr]
  simp only [sol, List.countP_cons, List.countP_nil, Nat.zero_add]
  split <;> rename_i h <;> simp [h]


/-- `Nat.sqrt` is well-founded recursion (no kernel reduction), so pin its concrete values via
    the characterization `m·m ≤ n < (m+1)·(m+1)`. -/
theorem sqrt_val {n m : ℕ} (h1 : m * m ≤ n) (h2 : n < (m + 1) * (m + 1)) : Nat.sqrt n = m :=
  le_antisymm (Nat.lt_succ_iff.mp (Nat.sqrt_lt.mpr h2)) (Nat.le_sqrt.mpr h1)

theorem isSq_9 : isSq 9 = true := by
  unfold isSq
  rw [sqrt_val (n := 9) (m := 3) (by norm_num) (by norm_num)]
  decide

theorem isSq_25 : isSq 25 = true := by
  unfold isSq
  rw [sqrt_val (n := 25) (m := 5) (by norm_num) (by norm_num)]
  decide

theorem isSq_18 : isSq 18 = false := by
  unfold isSq
  rw [sqrt_val (n := 18) (m := 4) (by norm_num) (by norm_num)]
  decide

/-- GROUND INSTANCE (official example 1): of the arrangements of [1,17,8] listed, exactly the two
    squareful ones ([1,8,17] and [17,8,1]) are counted — 1+8=9 and 8+17=25 are squares, 1+17 is
    not. -/
theorem vec : sol [[1, 17, 8], [1, 8, 17], [17, 8, 1]] = 2 := by
  norm_num [sol, List.countP_cons, List.countP_nil, adjSq, isSq_18, isSq_25, isSq_9]

end LC.P0996
