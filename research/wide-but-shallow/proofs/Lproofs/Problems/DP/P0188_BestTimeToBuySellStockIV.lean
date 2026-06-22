import Lproofs.Schemes.Fold

/-! @lc 188 | name:Best Time to Buy and Sell Stock IV | scheme:dp | family:dp-knapsack |
    complexity:O(nk) | source:https://leetcode.com/problems/best-time-to-buy-and-sell-stock-iv/

    Maximise profit with at most `k` non-overlapping buy/sell transactions. The accepted solution is a
    DP over (day, transactions-used, holding). CLASSIFICATION (dp): the value is a recursion over the
    transaction budget. CORRECTNESS (structure, not optimality): we model a trading plan as a list of
    disjoint increasing buy<sell intervals and certify the two facts the DP rests on --- doing nothing
    is always achievable (profit 0), and raising the transaction cap never loses an achievable profit
    (monotone in `k`). We do not prove the DP attains the maximum. -/

namespace LC.P0188

/-- Profit of a trading plan: sum of `sell − buy` price differences. -/
def profit (price : ℕ → ℤ) (plan : List (ℕ × ℕ)) : ℤ :=
  (plan.map fun t => price t.2 - price t.1).sum

/-- A valid plan: each transaction buys before it sells, and they are disjoint and increasing. -/
def valid : List (ℕ × ℕ) → Prop
  | [] => True
  | [t] => t.1 < t.2
  | a :: b :: rest => a.1 < a.2 ∧ a.2 ≤ b.1 ∧ valid (b :: rest)

/-- Profit `p` is achievable with at most `k` transactions. -/
def achievable (price : ℕ → ℤ) (k : ℕ) (p : ℤ) : Prop :=
  ∃ plan, valid plan ∧ plan.length ≤ k ∧ profit price plan = p

/-- SCHEME (dp): doing nothing (the empty plan) is always a valid zero-profit baseline --- the DP's
    base case. -/
theorem cls (price : ℕ → ℤ) (k : ℕ) : achievable price k 0 :=
  ⟨[], by trivial, Nat.zero_le k, rfl⟩

/-- CORRECT: raising the transaction cap never loses an achievable profit (monotone in `k`) --- the
    structural fact that makes the answer non-decreasing in `k` and lets it saturate. -/
theorem corr (price : ℕ → ℤ) (k : ℕ) (p : ℤ) (h : achievable price k p) :
    achievable price (k + 1) p := by
  obtain ⟨plan, hv, hlen, hp⟩ := h
  exact ⟨plan, hv, le_trans hlen (Nat.le_succ k), hp⟩

end LC.P0188
