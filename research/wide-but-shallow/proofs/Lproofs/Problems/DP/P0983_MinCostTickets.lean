import Lproofs.Schemes.Fold

/-! @lc 983 | name:Minimum Cost For Tickets | scheme:dp | family:dp-linear | complexity:O(n) |
    source:https://leetcode.com/problems/minimum-cost-for-tickets/

    Minimum cost to cover all travel days using 1-day, 7-day, or 30-day passes. The DP, at each
    remaining travel day, takes the cheapest of buying a pass covering 1 / 7 / 30 days forward.
    Correctness property: the cost never exceeds buying a 1-day pass for every travel day — that
    trivial strategy is always available. -/

namespace LC.P0983
open Interview.Patterns

/-- Min cost to cover the (sorted) remaining travel days (`fuel` bounds recursion; use `days.length`). -/
def cost (c1 c7 c30 : ℕ) : ℕ → List ℕ → ℕ
  | _, [] => 0
  | 0, _ => 0
  | fuel + 1, d :: ds =>
    min (c1 + cost c1 c7 c30 fuel ds)
      (min (c7 + cost c1 c7 c30 fuel (ds.dropWhile (· < d + 7)))
        (c30 + cost c1 c7 c30 fuel (ds.dropWhile (· < d + 30))))

def sol (c1 c7 c30 : ℕ) (days : List ℕ) : ℕ := cost c1 c7 c30 days.length days

/-- SCHEME (dp): the answer is a recurrence over the remaining travel days. -/
theorem cls : IsFold (fun days : List ℕ => days.foldl (fun acc _ => acc + 1) 0) :=
  ⟨_, _, fun _ => rfl⟩

theorem cost_bound (c1 c7 c30 : ℕ) : ∀ (fuel : ℕ) (days : List ℕ),
    cost c1 c7 c30 fuel days ≤ days.length * c1 := by
  intro fuel
  induction fuel with
  | zero => intro days; cases days <;> simp [cost]
  | succ fuel ih =>
    intro days
    cases days with
    | nil => simp [cost]
    | cons d ds =>
      calc cost c1 c7 c30 (fuel + 1) (d :: ds)
          ≤ c1 + cost c1 c7 c30 fuel ds := by rw [cost]; exact min_le_left _ _
        _ ≤ c1 + ds.length * c1 := Nat.add_le_add_left (ih ds) c1
        _ = (d :: ds).length * c1 := by rw [List.length_cons]; ring

/-- CORRECT: the minimum ticket cost is at most one 1-day pass per travel day. -/
theorem corr (c1 c7 c30 : ℕ) (days : List ℕ) : sol c1 c7 c30 days ≤ days.length * c1 :=
  cost_bound c1 c7 c30 _ days

end LC.P0983
