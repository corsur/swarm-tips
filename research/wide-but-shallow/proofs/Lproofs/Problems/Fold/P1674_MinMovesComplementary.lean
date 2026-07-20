import Lproofs.Schemes.Fold

/-! @lc 1674 | name:Minimum Moves to Make Array Complementary | scheme:fold | family:diff-array |
    complexity:O(n + limit) | source:https://leetcode.com/problems/minimum-moves-to-make-array-complementary/

    Each pair `(a, b) = (nums[i], nums[n-1-i])` must be made to sum to a common target `T`; every
    element may be changed to any value in `[1, limit]`. The accepted solution sweeps `T` with a
    difference array, where each pair contributes `0`, `1`, or `2` moves over ranges of `T`.
    CLASSIFICATION (fold): the per-`T` move total is a streaming tally over the pairs. CORRECTNESS (the
    per-pair range the diff-array marks, not the final minimum): we certify exactly when one change
    suffices for a pair---`T` is reachable in at most one change iff `T` lies in
    `[min(a,b)+1, max(a,b)+limit]`---which is the interval the accepted algorithm adds to. -/

namespace LC.P1674
open Interview.Patterns

/-- A target `T` is reachable for the pair `(a, b)` in at most one change: leave it (`T = a+b`), or
    change one element to some `v ∈ [1, limit]`. -/
def reach1 (limit a b T : ℤ) : Prop :=
  T = a + b ∨ ∃ v : ℤ, 1 ≤ v ∧ v ≤ limit ∧ (v + b = T ∨ a + v = T)

/-- The per-`T` sol contribution of one pair (the value the difference array stores). -/
def sol (limit a b T : ℤ) : ℕ :=
  if T = a + b then 0 else if min a b + 1 ≤ T ∧ T ≤ max a b + limit then 1 else 2

/-- SCHEME (fold): the total moves at a fixed target `T` is a streaming right-fold tally over the pairs. -/
theorem cls (limit T : ℤ) :
    IsRightFold (fun L : List (ℤ × ℤ) => (L.map (fun p => sol limit p.1 p.2 T)).sum) := by
  refine ⟨fun p acc => sol limit p.1 p.2 T + acc, 0, fun L => ?_⟩
  induction L with
  | nil => rfl
  | cons p t ih => simp [List.map_cons, List.sum_cons, ih, List.foldr_cons]

theorem reach1_range (limit a b T : ℤ) (ha1 : 1 ≤ a) (ha2 : a ≤ limit) (hb1 : 1 ≤ b)
    (hb2 : b ≤ limit) :
    reach1 limit a b T ↔ (min a b + 1 ≤ T ∧ T ≤ max a b + limit) := by
  rcases le_total a b with hab | hab <;>
    simp only [min_eq_left, min_eq_right, max_eq_left, max_eq_right, hab]
  · constructor
    · rintro (h | ⟨v, hv1, hv2, h | h⟩) <;> omega
    · rintro ⟨hlo, hhi⟩
      by_cases hc : T ≤ a + limit
      · exact Or.inr ⟨T - a, by omega, by omega, Or.inr (by omega)⟩
      · exact Or.inr ⟨T - b, by omega, by omega, Or.inl (by omega)⟩
  · constructor
    · rintro (h | ⟨v, hv1, hv2, h | h⟩) <;> omega
    · rintro ⟨hlo, hhi⟩
      by_cases hc : T ≤ b + limit
      · exact Or.inr ⟨T - b, by omega, by omega, Or.inl (by omega)⟩
      · exact Or.inr ⟨T - a, by omega, by omega, Or.inr (by omega)⟩

/-- CORRECT: one change suffices for the pair `(a, b)` to reach sum `T` exactly when its stored
    cost is at most 1 — the marking the accepted difference-array solution accumulates. -/
theorem corr (limit a b T : ℤ) (ha1 : 1 ≤ a) (ha2 : a ≤ limit) (hb1 : 1 ≤ b) (hb2 : b ≤ limit) :
    reach1 limit a b T ↔ sol limit a b T ≤ 1 := by
  rw [reach1_range limit a b T ha1 ha2 hb1 hb2]
  unfold sol
  split_ifs with h1 h2
  · exact ⟨fun _ => by norm_num, fun _ => by constructor <;> omega⟩
  · simp [h2]
  · exact ⟨fun h => absurd h h2, fun h => absurd h (by omega)⟩


/-- GROUND INSTANCE (official example 1, target sum 6): pair (1,3) needs one change, pair (2,4)
    already sums to 6 — total 1 move, the judge's answer. -/
theorem vec : sol 4 1 3 6 = 1 ∧ sol 4 2 4 6 = 0 := by decide

end LC.P1674
