import Lproofs.Schemes.Fold

/-! @lc 120 | name:Triangle | scheme:dp | family:dp-grid | complexity:O(n²) |
    source:https://leetcode.com/problems/triangle/

    The minimum top-to-bottom path sum, where each step descends to an adjacent entry below, is the
    interval DP `sol(i,j) = tri[i][j] + min(sol(i+1,j), sol(i+1,j+1))`. CLASSIFICATION (dp): a recursive
    decomposition picking the cheaper of two children (`cls`). CORRECTNESS: we certify the DP is sound
    against the actual paths — its value never exceeds the sum along any concrete descent (we exhibit the
    always-left path), so `sol` is a genuine lower-bounding minimum, not an arbitrary number. -/

namespace LC.P0120

/-- Minimum path sum over `f+1` rows starting at `(i, j)` (each step to an adjacent entry below). -/
def sol (tri : ℕ → ℕ → ℤ) : ℕ → ℕ → ℕ → ℤ
  | 0, i, j => tri i j
  | f + 1, i, j => tri i j + min (sol tri f (i + 1) j) (sol tri f (i + 1) (j + 1))

/-- The sum along one concrete descent: always step to the left child. -/
def leftSum (tri : ℕ → ℕ → ℤ) : ℕ → ℕ → ℕ → ℤ
  | 0, i, j => tri i j
  | f + 1, i, j => tri i j + leftSum tri f (i + 1) j

/-- SCHEME (dp): the value decomposes into the cell plus the cheaper of the two children. -/
theorem cls (tri : ℕ → ℕ → ℤ) (f i j : ℕ) :
    sol tri (f + 1) i j = tri i j + min (sol tri f (i + 1) j) (sol tri f (i + 1) (j + 1)) := rfl

/-- CORRECT: the DP minimum never exceeds the sum along a real descent (the always-left path), so it is
    a genuine lower-bounding path minimum. -/
theorem corr (tri : ℕ → ℕ → ℤ) (f i j : ℕ) : sol tri f i j ≤ leftSum tri f i j := by
  induction f generalizing i j with
  | zero => simp [sol, leftSum]
  | succ f ih =>
    simp only [sol, leftSum]
    have hmin : min (sol tri f (i + 1) j) (sol tri f (i + 1) (j + 1)) ≤ sol tri f (i + 1) j :=
      min_le_left _ _
    have := ih (i + 1) j
    linarith


/-- Official example 1 triangle [[2],[3,4],[6,5,7],[4,1,8,3]] (row i, entry j; off-triangle 0). -/
def exTri : ℕ → ℕ → ℤ := fun i j => ([[2], [3, 4], [6, 5, 7], [4, 1, 8, 3]].getD i []).getD j 0

/-- GROUND INSTANCE (official example 1): the minimum top-to-bottom path sum is 11 (2+3+5+1). -/
theorem vec : sol exTri 3 0 0 = 11 := by decide


/-- The sum along an arbitrary descent: at each step go to child `j` (false) or `j+1` (true). -/
def pathSum (tri : ℕ → ℕ → ℤ) : ℕ → ℕ → List Bool → ℤ
  | i, j, [] => tri i j
  | i, j, c :: cs => tri i j + pathSum tri (i + 1) (if c then j + 1 else j) cs

/-- ACHIEVABLE: the DP value is the sum along some real descent. -/
theorem achievable (tri : ℕ → ℕ → ℤ) :
    ∀ (f i j : ℕ), ∃ cs : List Bool, cs.length = f ∧ sol tri f i j = pathSum tri i j cs := by
  intro f
  induction f with
  | zero => exact fun i j => ⟨[], rfl, rfl⟩
  | succ f ih =>
    intro i j
    rcases le_total (sol tri f (i + 1) j) (sol tri f (i + 1) (j + 1)) with h | h
    · obtain ⟨cs, hlen, heq⟩ := ih (i + 1) j
      refine ⟨false :: cs, by simp [hlen], ?_⟩
      rw [show pathSum tri i j (false :: cs) = tri i j + pathSum tri (i + 1) j cs from by
        simp [pathSum]]
      simp only [sol]
      rw [min_eq_left h, heq]
    · obtain ⟨cs, hlen, heq⟩ := ih (i + 1) (j + 1)
      refine ⟨true :: cs, by simp [hlen], ?_⟩
      rw [show pathSum tri i j (true :: cs) = tri i j + pathSum tri (i + 1) (j + 1) cs from by
        simp [pathSum]]
      simp only [sol]
      rw [min_eq_right h, heq]

/-- OPTIMAL: the DP value lower-bounds the sum along every descent of the right length. With
    `achievable`, `sol` is exactly the minimum top-to-bottom path sum — full correctness. -/
theorem optimal (tri : ℕ → ℕ → ℤ) :
    ∀ (f i j : ℕ) (cs : List Bool), cs.length = f → sol tri f i j ≤ pathSum tri i j cs := by
  intro f
  induction f with
  | zero =>
    intro i j cs hcs
    rw [List.length_eq_zero_iff.mp hcs]
    exact le_refl _
  | succ f ih =>
    intro i j cs hcs
    match cs with
    | c :: cs' =>
      have hlen : cs'.length = f := by simpa using hcs
      cases c with
      | false =>
        calc sol tri (f + 1) i j ≤ tri i j + sol tri f (i + 1) j := by
              simp only [sol]; exact add_le_add le_rfl (min_le_left _ _)
          _ ≤ tri i j + pathSum tri (i + 1) j cs' := add_le_add le_rfl (ih (i + 1) j cs' hlen)
          _ = pathSum tri i j (false :: cs') := by simp [pathSum]
      | true =>
        calc sol tri (f + 1) i j ≤ tri i j + sol tri f (i + 1) (j + 1) := by
              simp only [sol]; exact add_le_add le_rfl (min_le_right _ _)
          _ ≤ tri i j + pathSum tri (i + 1) (j + 1) cs' :=
              add_le_add le_rfl (ih (i + 1) (j + 1) cs' hlen)
          _ = pathSum tri i j (true :: cs') := by simp [pathSum]

end LC.P0120
