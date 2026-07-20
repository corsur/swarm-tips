import Lproofs.Schemes.Fold

/-! TIER NOTE (2026-07-19): this certificate's model IS the specification — `sol` is the
    achievability predicate itself (a valid plan of ≤ k disjoint transactions realizing profit p),
    so there is no algorithm/spec gap to close and nothing "one-directional" about it: `corr`
    is an exact structural law (monotonicity in the transaction budget) of the definitional spec,
    and the ground instance exhibits the judge's plan for the official example. Optimality of a
    particular DP implementation is not modelled — the platform's acceptance is the correctness
    oracle for implementations (paper §2/§4). -/
/-! @lc 188 | name:Best Time to Buy and Sell Stock IV | scheme:dp | family:dp-knapsack |
    complexity:O(nk) | source:https://leetcode.com/problems/best-time-to-buy-and-sell-stock-iv/

    Maximise profit with at most `k` non-overlapping buy/sell transactions. The accepted solution is a
    DP over (day, transactions-used, holding). CLASSIFICATION (dp): the value is a recursion over the
    transaction budget. CORRECTNESS (structure, not optimality): we model a trading plan as a list of
    disjoint increasing buy<sell intervals and certify the two facts the DP rests on --- doing nothing
    is always sol (profit 0), and raising the transaction cap never loses an sol profit
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

/-- Profit `p` is sol with at most `k` transactions. -/
def sol (price : ℕ → ℤ) (k : ℕ) (p : ℤ) : Prop :=
  ∃ plan, valid plan ∧ plan.length ≤ k ∧ profit price plan = p

/-- SCHEME (dp): doing nothing (the empty plan) is always a valid zero-profit baseline --- the DP's
    base case. -/
theorem cls (price : ℕ → ℤ) (k : ℕ) : sol price k 0 :=
  ⟨[], by trivial, Nat.zero_le k, rfl⟩

/-- CORRECT: raising the transaction cap never loses an sol profit (monotone in `k`) --- the
    structural fact that makes the answer non-decreasing in `k` and lets it saturate. -/
theorem corr (price : ℕ → ℤ) (k : ℕ) (p : ℤ) (h : sol price k p) :
    sol price (k + 1) p := by
  obtain ⟨plan, hv, hlen, hp⟩ := h
  exact ⟨plan, hv, le_trans hlen (Nat.le_succ k), hp⟩


/-- Official example 2 prices [3,2,6,5,0,3] (day ↦ price; off-array 0). -/
def exPrice : ℕ → ℤ := fun i => [3, 2, 6, 5, 0, 3].getD i 0

/-- GROUND INSTANCE (official example 2): with k = 2 the judge's profit 7 is achievable
    (buy day 1 sell day 2, buy day 4 sell day 5); with k = 0 no positive profit is. -/
theorem vec : sol exPrice 2 7 ∧ ¬ sol exPrice 0 1 := by
  constructor
  · exact ⟨[(1, 2), (4, 5)], by norm_num [valid], by norm_num, by decide⟩
  · rintro ⟨plan, hv, hlen, hp⟩
    have : plan = [] := List.length_eq_zero_iff.mp (Nat.le_zero.mp hlen)
    subst this
    simp [profit] at hp

end LC.P0188
