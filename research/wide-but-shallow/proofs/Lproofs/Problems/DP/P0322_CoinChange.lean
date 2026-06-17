import Lproofs.Schemes.Fold

/-! @lc 322 | name:Coin Change | scheme:dp | family:knapsack | complexity:O(amount·coins) |
    source:https://leetcode.com/problems/coin-change/

    Fewest coins summing to `amount` (coins reusable). CLASSIFICATION: the same recursive decomposition
    as Combination Sum — enumerate coin multisets summing to `amount` by taking the head coin (recurse
    with `amount - c`, prepend it) or dropping it; the answer is the minimum length over those (the
    `min` is the optimality we DROP). NON-VACUITY: every enumerated multiset provably sums to `amount`,
    pinning the decomposition to real subproblem work. We certify the decomposition + soundness. -/

namespace LC.P0322

/-- Coin multisets summing to `t` from `coins` (reusable); `fuel` bounds the recursion depth. -/
def ways : ℕ → List ℕ → ℕ → List (List ℕ)
  | 0, _, _ => []
  | _ + 1, _, 0 => [[]]
  | _ + 1, [], _ => []
  | fuel + 1, c :: cs, t + 1 =>
    (if c ≤ t + 1 then (ways fuel (c :: cs) (t + 1 - c)).map (c :: ·) else []) ++
      ways fuel cs (t + 1)

/-- The accepted answer: the fewest coins over all ways (the `min` is the dropped optimality). -/
def sol (coins : List ℕ) (amount : ℕ) : Option ℕ :=
  ((ways (amount + 1) coins amount).map List.length).min?

/-- SCHEME (dp / recursive decomposition): take the head coin (recurse with `amount - c`, prepend it)
    or drop it (recurse on the remaining coins) — the genuine subproblem split. -/
theorem cls (fuel : ℕ) (c : ℕ) (cs : List ℕ) (t : ℕ) :
    ways (fuel + 1) (c :: cs) (t + 1) =
      (if c ≤ t + 1 then (ways fuel (c :: cs) (t + 1 - c)).map (c :: ·) else []) ++
        ways fuel cs (t + 1) :=
  rfl

/-- NON-VACUITY (soundness): every enumerated coin multiset sums to the target amount. -/
theorem corr : ∀ (fuel : ℕ) (coins : List ℕ) (t : ℕ) (w : List ℕ),
    w ∈ ways fuel coins t → w.sum = t := by
  intro fuel
  induction fuel with
  | zero => intro coins t w hmem; simp [ways] at hmem
  | succ f ih =>
    intro coins t w hmem
    rcases t with _ | t
    · simp only [ways, List.mem_singleton] at hmem; subst hmem; rfl
    · rcases coins with _ | ⟨c, cs⟩
      · simp [ways] at hmem
      · rw [cls, List.mem_append] at hmem
        rcases hmem with h | h
        · split at h
          · rw [List.mem_map] at h
            obtain ⟨w', hmem', rfl⟩ := h
            have hc' := ih (c :: cs) (t + 1 - c) w' hmem'
            rw [List.sum_cons, hc']
            omega
          · simp at h
        · exact ih cs (t + 1) w h

end LC.P0322
