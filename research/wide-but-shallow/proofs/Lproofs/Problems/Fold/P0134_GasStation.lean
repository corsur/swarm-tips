import Lproofs.Patterns

/-! @lc 134 | name:Gas Station | scheme:fold | family:running-balance | complexity:O(n) |
    source:https://leetcode.com/problems/gas-station/

    Drive a circular route; at station `i` you gain `gas[i] - cost[i]`. CLASSIFICATION: the feasibility
    test is a streaming left fold over the per-station net differences, accumulating the running tank
    balance (a scalar). A full loop is possible iff the total net is non-negative. NON-VACUITY: we prove
    the fold's running balance equals the prefix sum of the differences — genuine incremental tank
    state, accumulated linearly in the seed (defeats carry-the-input vacuity). We certify the fold + the
    running-balance recurrence; DROP the concrete start-index output. -/

namespace LC.P0134
open Interview.Patterns

/-- Running tank balance: fold the per-station net differences. -/
def total (ds : List ℤ) : ℤ := ds.foldl (· + ·) 0

/-- SCHEME (fold): the running balance is a left fold with a scalar accumulator. -/
theorem cls : IsFold (fun ds : List ℤ => ds.foldl (· + ·) 0) := ⟨(· + ·), 0, fun _ => rfl⟩

/-- The balance accumulates linearly in its seed — genuine incremental tank state. -/
theorem foldl_add (ds : List ℤ) (c : ℤ) : ds.foldl (· + ·) c = c + ds.foldl (· + ·) 0 := by
  induction ds generalizing c with
  | nil => simp
  | cons d ds ih => simp only [List.foldl_cons, zero_add]; rw [ih (c + d), ih d]; ring

/-- NON-VACUITY: the running balance over `d :: ds` is `d` plus the balance over the rest — the tank
    is updated one station at a time, a real prefix-sum recurrence. -/
theorem corr (d : ℤ) (ds : List ℤ) : total (d :: ds) = d + total ds := by
  simp only [total, List.foldl_cons, zero_add]
  exact foldl_add ds d

end LC.P0134
