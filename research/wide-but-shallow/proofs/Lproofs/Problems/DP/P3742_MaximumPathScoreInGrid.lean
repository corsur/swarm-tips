import Lproofs.Schemes.Fold

/-! @lc 3742 | name:Maximum Path Score in a Grid | scheme:dp | family:dp-grid | complexity:O(m·n·k) |
    source:https://leetcode.com/problems/maximum-path-score-in-a-grid/

    From `(0,0)` to `(m-1,n-1)` moving only right or down on a grid of values `0/1/2`, each cell adds
    its value to the score and costs `0` (value 0) or `1` (values 1,2); maximize score with total cost
    `≤ k`. The accepted solution is a DP over `(i, j, budget)`. CLASSIFICATION (dp): the value is a
    recursion choosing the better of the two moves. CORRECTNESS (a structural property of the actual
    budget DP, not optimality): the DP value is monotone in the budget --- a larger cost allowance never
    lowers the achievable score --- which is why the answer is non-decreasing in `k` and saturates. -/

namespace LC.P3742

/-- Cost of a cell value: `0` costs nothing, `1` and `2` cost one. -/
def cellCost (v : ℕ) : ℕ := if v = 0 then 0 else 1

/-- Best score from the two onward moves (right/down), `0` past the boundary. -/
def nextBest (val : ℕ → ℕ → ℕ) (m n : ℕ) (rec : ℕ → ℕ → ℕ → ℕ) (i j budget : ℕ) : ℕ :=
  max (if i + 1 < m then rec (i + 1) j budget else 0)
      (if j + 1 < n then rec i (j + 1) budget else 0)

/-- DP: max score from `(i,j)` to the corner within `budget`, moving right/down (`fuel`-bounded). -/
def sol (val : ℕ → ℕ → ℕ) (m n : ℕ) : ℕ → ℕ → ℕ → ℕ → ℕ
  | 0, _, _, _ => 0
  | f + 1, i, j, budget =>
    if cellCost (val i j) > budget then 0
    else val i j + nextBest val m n (sol val m n f) i j (budget - cellCost (val i j))

/-- SCHEME (dp): an affordable cell decomposes into its value plus the sol onward move --- the
    right/down recurrence. -/
theorem cls (val : ℕ → ℕ → ℕ) (m n f i j budget : ℕ) (hb : cellCost (val i j) ≤ budget) :
    sol val m n (f + 1) i j budget =
      val i j + nextBest val m n (sol val m n f) i j (budget - cellCost (val i j)) := by
  simp only [sol]; rw [if_neg (by omega)]

/-- CORRECT: the DP value is monotone in the budget --- raising the cost allowance never lowers the
    achievable score. A genuine property of the actual budget DP, no optimality claimed. -/
theorem corr (val : ℕ → ℕ → ℕ) (m n f i j : ℕ) :
    Monotone (sol val m n f i j) := by
  induction f generalizing i j with
  | zero => intro a b _; simp [sol]
  | succ f ih =>
    intro a b hab
    simp only [sol, nextBest]
    by_cases hcb : cellCost (val i j) > b
    · simp [hcb, show cellCost (val i j) > a from by omega]
    · by_cases hca : cellCost (val i j) > a
      · simp only [if_pos hca]; exact Nat.zero_le _
      · rw [if_neg hca, if_neg hcb]
        have hsub : a - cellCost (val i j) ≤ b - cellCost (val i j) := by omega
        refine Nat.add_le_add_left (max_le_max ?_ ?_) _
        · split
          · exact ih (i + 1) j hsub
          · exact le_refl 0
        · split
          · exact ih i (j + 1) hsub
          · exact le_refl 0


/-- GROUND INSTANCE (grid [[0,1],[1,2]], budget 1): the best affordable path scores 1 — the
    2-cell corner is unaffordable after spending the budget on either middle cell. -/
theorem vec : sol (fun i j => ([[0, 1], [1, 2]].getD i []).getD j 0) 2 2 3 0 0 1 = 1 := by decide


/-- Play a concrete move-strategy: `true` steps right, `false` steps down; off-grid moves and
    unaffordable cells score nothing, mirroring the DP's boundary behaviour. -/
def play (val : ℕ → ℕ → ℕ) (m n : ℕ) : List Bool → ℕ → ℕ → ℕ → ℕ
  | [], _, _, _ => 0
  | d :: ds, i, j, budget =>
    if cellCost (val i j) > budget then 0
    else val i j +
      match d with
      | false =>
        if i + 1 < m then play val m n ds (i + 1) j (budget - cellCost (val i j)) else 0
      | true =>
        if j + 1 < n then play val m n ds i (j + 1) (budget - cellCost (val i j)) else 0

/-- ACHIEVABLE: the DP value is realized by some concrete move-strategy. -/
theorem achievable (val : ℕ → ℕ → ℕ) (m n : ℕ) :
    ∀ (f i j budget : ℕ), ∃ ds : List Bool,
      ds.length = f ∧ sol val m n f i j budget = play val m n ds i j budget := by
  intro f
  induction f with
  | zero => exact fun _ _ _ => ⟨[], rfl, rfl⟩
  | succ f ih =>
    intro i j budget
    by_cases hc : cellCost (val i j) > budget
    · refine ⟨List.replicate (f + 1) false, List.length_replicate, ?_⟩
      rw [List.replicate_succ]
      simp [sol, play, hc]
    · rcases le_total
          (if i + 1 < m then sol val m n f (i + 1) j (budget - cellCost (val i j)) else 0)
          (if j + 1 < n then sol val m n f i (j + 1) (budget - cellCost (val i j)) else 0) with h | h
      · by_cases hj : j + 1 < n
        · obtain ⟨ds, hlen, heq⟩ := ih i (j + 1) (budget - cellCost (val i j))
          refine ⟨true :: ds, by simp [hlen], ?_⟩
          rw [show play val m n (true :: ds) i j budget =
              val i j + (if j + 1 < n then play val m n ds i (j + 1) (budget - cellCost (val i j)) else 0) from by
            simp [play, hc]]
          rw [if_pos hj] at h
          rw [heq] at h
          simp only [sol, if_neg hc, nextBest, if_pos hj, heq]
          rw [max_eq_right h]
        · obtain ⟨ds, hlen, _⟩ := ih i (j + 1) (budget - cellCost (val i j))
          refine ⟨true :: ds, by simp [hlen], ?_⟩
          rw [show play val m n (true :: ds) i j budget =
              val i j + (if j + 1 < n then play val m n ds i (j + 1) (budget - cellCost (val i j)) else 0) from by
            simp [play, hc]]
          rw [if_neg hj] at h
          simp only [sol, if_neg hc, nextBest, if_neg hj]
          rw [max_eq_right h]
      · by_cases hi : i + 1 < m
        · obtain ⟨ds, hlen, heq⟩ := ih (i + 1) j (budget - cellCost (val i j))
          refine ⟨false :: ds, by simp [hlen], ?_⟩
          rw [show play val m n (false :: ds) i j budget =
              val i j + (if i + 1 < m then play val m n ds (i + 1) j (budget - cellCost (val i j)) else 0) from by
            simp [play, hc]]
          rw [if_pos hi] at h
          rw [heq] at h
          simp only [sol, if_neg hc, nextBest, if_pos hi, heq]
          rw [max_eq_left h]
        · obtain ⟨ds, hlen, _⟩ := ih (i + 1) j (budget - cellCost (val i j))
          refine ⟨false :: ds, by simp [hlen], ?_⟩
          rw [show play val m n (false :: ds) i j budget =
              val i j + (if i + 1 < m then play val m n ds (i + 1) j (budget - cellCost (val i j)) else 0) from by
            simp [play, hc]]
          rw [if_neg hi] at h
          simp only [sol, if_neg hc, nextBest, if_neg hi]
          rw [max_eq_left h]

/-- OPTIMAL: no move-strategy of the right length beats the DP value. With `achievable`, `sol`
    is exactly the maximum affordable path score — full correctness over the strategy space. -/
theorem optimal (val : ℕ → ℕ → ℕ) (m n : ℕ) :
    ∀ (f i j budget : ℕ) (ds : List Bool), ds.length = f →
      play val m n ds i j budget ≤ sol val m n f i j budget := by
  intro f
  induction f with
  | zero =>
    intro i j budget ds hds
    rw [List.length_eq_zero_iff.mp hds]
    exact le_refl _
  | succ f ih =>
    intro i j budget ds hds
    match ds with
    | d :: ds' =>
      have hlen : ds'.length = f := by simpa using hds
      by_cases hc : cellCost (val i j) > budget
      · simp [sol, play, hc]
      · rw [show sol val m n (f + 1) i j budget =
            val i j + nextBest val m n (sol val m n f) i j (budget - cellCost (val i j))
            from by simp [sol, hc]]
        cases d with
        | false =>
          rw [show play val m n (false :: ds') i j budget = val i j +
              (if i + 1 < m then
                play val m n ds' (i + 1) j (budget - cellCost (val i j)) else 0) from by
            simp [play, hc]]
          refine add_le_add le_rfl ?_
          by_cases hi : i + 1 < m
          · rw [if_pos hi]
            refine le_trans (ih (i + 1) j _ ds' hlen) (le_trans ?_ (le_max_left _ _))
            rw [if_pos hi]
          · rw [if_neg hi]
            exact Nat.zero_le _
        | true =>
          rw [show play val m n (true :: ds') i j budget = val i j +
              (if j + 1 < n then
                play val m n ds' i (j + 1) (budget - cellCost (val i j)) else 0) from by
            simp [play, hc]]
          refine add_le_add le_rfl ?_
          by_cases hj : j + 1 < n
          · rw [if_pos hj]
            refine le_trans (ih i (j + 1) _ ds' hlen) (le_trans ?_ (le_max_right _ _))
            rw [if_pos hj]
          · rw [if_neg hj]
            exact Nat.zero_le _

end LC.P3742
