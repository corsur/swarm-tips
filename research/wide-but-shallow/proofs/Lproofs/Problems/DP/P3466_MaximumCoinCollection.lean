import Lproofs.Schemes.Fold

/-! @lc 3466 | name:Maximum Coin Collection | scheme:dp | family:dp-grid | complexity:O(n) |
    source:https://leetcode.com/problems/maximum-coin-collection/

    Mario drives a two-lane freeway, starting in lane 1, collecting coins and switching lanes at most
    twice; maximize coins. The accepted solution is a DP `dfs(i, lane, switchesLeft)`. CLASSIFICATION
    (dp): the value is a recursion choosing to stay or (if switches remain) switch lanes. CORRECTNESS (a
    structural property of the actual DP, not optimality): the value is monotone in the remaining lane
    switches --- having more switches available never lowers the coins collected. -/

namespace LC.P3466

/-- The other of the two lanes. -/
def other (lane : ℕ) : ℕ := 1 - lane

/-- Best onward coins: stay in lane, or (if a switch remains) switch lanes spending one. -/
def nextMove (coin : ℕ → ℕ → ℤ) (rec : ℕ → ℕ → ℕ → ℤ) (pos lane sw : ℕ) : ℤ :=
  max (rec (pos + 1) lane sw)
      (if sw > 0 then rec (pos + 1) (other lane) (sw - 1) else rec (pos + 1) lane sw)

/-- DP: max coins from `pos` in `lane` with `sw` switches left, over `len` positions (`fuel`-bounded). -/
def sol (coin : ℕ → ℕ → ℤ) (len : ℕ) : ℕ → ℕ → ℕ → ℕ → ℤ
  | 0, _, _, _ => 0
  | f + 1, pos, lane, sw =>
    if pos ≥ len then 0 else coin pos lane + nextMove coin (sol coin len f) pos lane sw

/-- SCHEME (dp): an in-range cell decomposes into its coins plus the sol onward move --- the
    stay/switch recurrence. -/
theorem cls (coin : ℕ → ℕ → ℤ) (len f pos lane sw : ℕ) (h : pos < len) :
    sol coin len (f + 1) pos lane sw =
      coin pos lane + nextMove coin (sol coin len f) pos lane sw := by
  simp only [sol]; rw [if_neg (by omega)]

/-- CORRECT: the value is monotone in the remaining switches --- more switches never lower the coins
    collected. A genuine property of the actual DP, no optimality claimed. -/
theorem corr (coin : ℕ → ℕ → ℤ) (len f pos lane : ℕ) :
    Monotone (sol coin len f pos lane) := by
  induction f generalizing pos lane with
  | zero => intro a b _; simp [sol]
  | succ f ih =>
    intro a b hab
    simp only [sol]
    by_cases hp : pos ≥ len
    · simp [hp]
    · rw [if_neg hp, if_neg hp]
      gcongr
      simp only [nextMove]
      refine max_le ?_ ?_
      · exact le_trans (ih (pos + 1) lane hab) (le_max_left _ _)
      · by_cases ha : a > 0
        · rw [if_pos ha]
          have hb : b > 0 := by omega
          refine le_trans (ih (pos + 1) (other lane) (show a - 1 ≤ b - 1 by omega)) ?_
          rw [if_pos hb]; exact le_max_right _ _
        · rw [if_neg ha]
          exact le_trans (ih (pos + 1) lane hab) (le_max_left _ _)


/-- GROUND INSTANCE (lane coins [[1,2],[10,20]], 2 positions): with one switch left, take lane
    0's coin then switch — 1 + 20 = 21; with none, stay for 1 + 2 = 3. -/
theorem vec : sol (fun p l => ([[1, 2], [10, 20]].getD l []).getD p 0) 2 3 0 0 1 = 21 ∧
    sol (fun p l => ([[1, 2], [10, 20]].getD l []).getD p 0) 2 3 0 0 0 = 3 := by decide

end LC.P3466
