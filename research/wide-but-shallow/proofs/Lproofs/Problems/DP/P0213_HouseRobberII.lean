import Lproofs.Problems.DP.P0198_HouseRobber

/-! @lc 213 | name:House Robber II | scheme:dp | family:dp-linear | complexity:O(n) |
    source:https://leetcode.com/problems/house-robber-ii/

    Houses arranged in a circle: house 0 and house n-1 are also adjacent. The maximum is the
    larger of two linear House-Robber sub-problems — drop the last house, or drop the first —
    because a valid circular selection can never take BOTH endpoints, so it omits one of them.
    Correctness reuses `LC.P0198` (linear House Robber) as the ACHIEVABLE+OPTIMAL oracle on each
    sub-array, and proves the circular reduction faithfully against a cyclic-validity spec. -/

namespace LC.P0213
open Interview.Patterns
open scoped List

/-- Circular non-adjacency: linearly valid, and (for ≥ 2 houses) not both endpoints robbed. -/
def NoAdjCyc (s : List Bool) : Prop :=
  LC.P0198.NoAdj s ∧ (2 ≤ s.length → ¬(s.headI = true ∧ s.getLastD false = true))

/-- A circular selection aligned with the houses. -/
def SelCyc (a : List ℤ) (s : List Bool) : Prop := s.length = a.length ∧ NoAdjCyc s

/-- `v` is achievable by a circular-valid selection. -/
def CycRob (a : List ℤ) (v : ℤ) : Prop := ∃ s, SelCyc a s ∧ LC.P0198.selSum a s = v

/-- Answer: a single house if `n ≤ 1`, else the better of "drop last" and "drop first". -/
def sol (a : List ℤ) : ℤ :=
  if a.length ≤ 1 then LC.P0198.sol a
  else max (LC.P0198.sol a.dropLast) (LC.P0198.sol a.tail)

-- A leading non-robbed house drops out of the total / constraint.
theorem selSum_cons_false (x : ℤ) (a : List ℤ) (s : List Bool) :
    LC.P0198.selSum (x :: a) (false :: s) = LC.P0198.selSum a s := by
  simp [LC.P0198.selSum]

theorem naFold_cons_false (s : List Bool) :
    LC.P0198.naFold (false :: s) = LC.P0198.naFold s := by
  simp [LC.P0198.naFold, List.foldl_cons]

theorem noAdj_cons_false {s : List Bool} : LC.P0198.NoAdj (false :: s) ↔ LC.P0198.NoAdj s := by
  unfold LC.P0198.NoAdj; rw [naFold_cons_false]

theorem headI_false_split {s : List Bool} (h : s.headI = false) (hne : s ≠ []) :
    ∃ u, s = false :: u := by
  cases s with
  | nil => exact absurd rfl hne
  | cons b u => simp only [List.headI] at h; exact ⟨u, by rw [h]⟩

/-- For `n ≤ 1`, the circular constraint is vacuous, so circular = linear selections. -/
theorem cycRob_iff_rob_small {a : List ℤ} (ha : a.length ≤ 1) (v : ℤ) :
    CycRob a v ↔ LC.P0198.Rob a v := by
  constructor
  · rintro ⟨s, ⟨hlen, hna, _⟩, hsum⟩; exact ⟨s, ⟨hlen, hna⟩, hsum⟩
  · rintro ⟨s, ⟨hlen, hna⟩, hsum⟩
    exact ⟨s, ⟨hlen, hna, fun h2 => absurd h2 (by omega)⟩, hsum⟩

/-- A trailing non-robbed house: the total reduces to the linear sub-problem on `dropLast`. -/
theorem selSum_dropLast_concat_false {a : List ℤ} (hne : a ≠ []) {w : List Bool}
    (hw : (a.dropLast).length = w.length) :
    LC.P0198.selSum a (w ++ [false]) = LC.P0198.selSum a.dropLast w := by
  conv_lhs => rw [← List.dropLast_concat_getLast hne]
  rw [LC.P0198.selSum_concat_false hw]

/-- A leading non-robbed house: the total reduces to the linear sub-problem on `tail`. -/
theorem selSum_tail_cons_false {a : List ℤ} (hne : a ≠ []) (u : List Bool) :
    LC.P0198.selSum a (false :: u) = LC.P0198.selSum a.tail u := by
  obtain ⟨a0, arest, rfl⟩ := List.exists_cons_of_ne_nil hne
  rw [selSum_cons_false, List.tail_cons]

/-- If the last bit is unset, the selection ends in an explicit `false`. -/
theorem getLastD_false_concat {s : List Bool} (hne : s ≠ []) (h : s.getLastD false = false) :
    s = s.dropLast ++ [false] := by
  have hgl : s.getLast hne = false := by
    simp only [List.getLastD_eq_getLast?, List.getLast?_eq_getLast hne, Option.getD_some] at h
    exact h
  conv_lhs => rw [← List.dropLast_concat_getLast hne]
  rw [hgl]

/-- Lift a linear solution on `dropLast` to a circular selection (drop the last house). -/
theorem lift_dropLast {a : List ℤ} (hne : a ≠ []) {v : ℤ}
    (h : LC.P0198.Rob a.dropLast v) : CycRob a v := by
  obtain ⟨w, ⟨hwlen, hwna⟩, hwsum⟩ := h
  have hpos : 0 < a.length := List.length_pos_iff.mpr hne
  refine ⟨w ++ [false], ⟨?_, LC.P0198.noAdj_false hwna, ?_⟩, ?_⟩
  · simp only [List.length_append, List.length_singleton, hwlen, List.length_dropLast]; omega
  · intro _ hcon
    obtain ⟨_, hl⟩ := hcon
    simp [List.getLastD_eq_getLast?, List.getLast?_concat] at hl
  · rw [selSum_dropLast_concat_false hne hwlen.symm]; exact hwsum

/-- Lift a linear solution on `tail` to a circular selection (drop the first house). -/
theorem lift_tail {a : List ℤ} (hne : a ≠ []) {v : ℤ}
    (h : LC.P0198.Rob a.tail v) : CycRob a v := by
  obtain ⟨u, ⟨hulen, huna⟩, husum⟩ := h
  have hpos : 0 < a.length := List.length_pos_iff.mpr hne
  refine ⟨false :: u, ⟨?_, noAdj_cons_false.mpr huna, ?_⟩, ?_⟩
  · simp only [List.length_cons, hulen, List.length_tail]; omega
  · intro _ hcon; obtain ⟨hh, _⟩ := hcon; simp [List.headI] at hh
  · rw [selSum_tail_cons_false hne]; exact husum

/-- SCHEME (dp): each sub-problem is the House Robber streaming fold (the recurrence). -/
theorem cls : IsFold (fun a : List ℤ =>
    a.foldl (fun st y => (max st.1 (st.2 + y), st.1)) ((0 : ℤ), (0 : ℤ))) :=
  ⟨_, _, fun _ => rfl⟩

/-- CORRECT: `sol` is the sum of a real circular-valid selection (ACHIEVABLE) and an upper bound
    on every such selection (OPTIMAL) — i.e. the maximum non-adjacent-on-a-circle subset sum. -/
theorem corr (a : List ℤ) : CycRob a (sol a) ∧ ∀ v, CycRob a v → v ≤ sol a := by
  by_cases ha : a.length ≤ 1
  · have hsol : sol a = LC.P0198.sol a := by rw [sol, if_pos ha]
    rw [hsol]
    have hlin := LC.P0198.corr a
    exact ⟨(cycRob_iff_rob_small ha _).mpr hlin.1,
      fun v hv => hlin.2 v ((cycRob_iff_rob_small ha v).mp hv)⟩
  · have ha2 : 2 ≤ a.length := by omega
    have hne : a ≠ [] := by intro h; rw [h] at ha2; simp at ha2
    have hsol : sol a = max (LC.P0198.sol a.dropLast) (LC.P0198.sol a.tail) := by
      rw [sol, if_neg (by omega)]
    rw [hsol]
    refine ⟨?_, ?_⟩
    · rcases max_choice (LC.P0198.sol a.dropLast) (LC.P0198.sol a.tail) with hc | hc <;> rw [hc]
      · exact lift_dropLast hne (LC.P0198.corr a.dropLast).1
      · exact lift_tail hne (LC.P0198.corr a.tail).1
    · rintro v ⟨s, ⟨hlen, hna, hwrap⟩, hsum⟩
      have hsne : s ≠ [] := by intro h; rw [h] at hlen; simp at hlen; omega
      have hs2 : 2 ≤ s.length := by rw [hlen]; exact ha2
      by_cases hlast : s.getLastD false = true
      · have hhead : s.headI = false := by
          by_contra hh
          exact hwrap hs2 ⟨by simpa using hh, hlast⟩
        obtain ⟨u, rfl⟩ := headI_false_split hhead hsne
        rw [selSum_tail_cons_false hne] at hsum
        have hrob : LC.P0198.Rob a.tail v :=
          ⟨u, ⟨by rw [List.length_tail]; simp only [List.length_cons] at hlen; omega,
            noAdj_cons_false.mp hna⟩, hsum⟩
        exact le_trans ((LC.P0198.corr a.tail).2 v hrob) (le_max_right _ _)
      · have hlastf : s.getLastD false = false := by
          cases hg : s.getLastD false with
          | false => rfl
          | true => exact absurd hg hlast
        have hsd := getLastD_false_concat hsne hlastf
        rw [hsd] at hsum hna
        have hwlen : (a.dropLast).length = s.dropLast.length := by
          rw [List.length_dropLast, List.length_dropLast, hlen]
        rw [selSum_dropLast_concat_false hne hwlen] at hsum
        have hrob : LC.P0198.Rob a.dropLast v :=
          ⟨s.dropLast, ⟨hwlen.symm, LC.P0198.noAdj_false_inv hna⟩, hsum⟩
        exact le_trans ((LC.P0198.corr a.dropLast).2 v hrob) (le_max_left _ _)

end LC.P0213
