import Lproofs.Schemes.Fold

/-! @lc 198 | name:House Robber | scheme:dp | family:dp-linear | complexity:O(n) |
    source:https://leetcode.com/problems/house-robber/

    Maximum non-adjacent subset sum. State `(f, g)`: `f` is the best total over any valid
    (no two adjacent) selection of the processed prefix, `g` the best over selections that do
    NOT take the last house. Recurrence `f' = max f (g + y)`, `g' = f`. Correctness is the
    gold-standard pair — ACHIEVABLE (the answer is a real valid selection's total) and OPTIMAL
    (no valid selection beats it). A selection is a `List Bool` aligned with the houses; the
    no-two-adjacent constraint is a left fold tracking the previous bit. -/

namespace LC.P0198
open Interview.Patterns

/-- Fold checking "no two adjacent `true`s", carrying `(ok, lastBit)`. -/
def naFold (s : List Bool) : Bool × Bool :=
  s.foldl (fun st b => (st.1 && !(st.2 && b), b)) (true, false)

/-- A valid (no two adjacent robbed houses) selection. -/
def NoAdj (s : List Bool) : Prop := (naFold s).1 = true

/-- The selection does not rob its last house (vacuously true when empty). -/
def EndFalse (s : List Bool) : Prop := (naFold s).2 = false

/-- Total robbed by selection `s` over houses `a`. -/
def selSum (a : List ℤ) (s : List Bool) : ℤ :=
  (List.zipWith (fun x b => if b then x else 0) a s).sum

/-- A selection aligned with the houses and respecting the adjacency constraint. -/
def Sel (a : List ℤ) (s : List Bool) : Prop := s.length = a.length ∧ NoAdj s

/-- `v` is achievable as some valid selection's total. -/
def Rob (a : List ℤ) (v : ℤ) : Prop := ∃ s, Sel a s ∧ selSum a s = v

/-- `v` is achievable by a valid selection that does not rob the last house. -/
def RobNL (a : List ℤ) (v : ℤ) : Prop := ∃ s, Sel a s ∧ EndFalse s ∧ selSum a s = v

/-- Kadane-style invariant for the processed prefix `p`. -/
structure Valid (p : List ℤ) (f g : ℤ) : Prop where
  fAch : Rob p f
  fOpt : ∀ v, Rob p v → v ≤ f
  gAch : RobNL p g
  gOpt : ∀ v, RobNL p v → v ≤ g

def step (st : ℤ × ℤ) (y : ℤ) : ℤ × ℤ := (max st.1 (st.2 + y), st.1)

def sol (a : List ℤ) : ℤ := (a.foldl step (0, 0)).1

-- snoc behaviour of the constraint fold
theorem naFold_fst_concat (s : List Bool) (b : Bool) :
    (naFold (s ++ [b])).1 = ((naFold s).1 && !((naFold s).2 && b)) := by
  simp only [naFold, List.foldl_append, List.foldl_cons, List.foldl_nil]

theorem naFold_snd_concat (s : List Bool) (b : Bool) : (naFold (s ++ [b])).2 = b := by
  simp only [naFold, List.foldl_append, List.foldl_cons, List.foldl_nil]

theorem noAdj_false {s : List Bool} (h : NoAdj s) : NoAdj (s ++ [false]) := by
  unfold NoAdj at *; rw [naFold_fst_concat]; simp [h]

theorem noAdj_false_inv {s : List Bool} (h : NoAdj (s ++ [false])) : NoAdj s := by
  unfold NoAdj at *; rw [naFold_fst_concat] at h; simpa using h

theorem endFalse_false (s : List Bool) : EndFalse (s ++ [false]) := by
  unfold EndFalse; rw [naFold_snd_concat]

theorem noAdj_true {s : List Bool} (h : NoAdj s) (he : EndFalse s) : NoAdj (s ++ [true]) := by
  unfold NoAdj EndFalse at *; rw [naFold_fst_concat]; simp [h, he]

theorem noAdj_true_inv {s : List Bool} (h : NoAdj (s ++ [true])) : NoAdj s ∧ EndFalse s := by
  unfold NoAdj EndFalse at *; rw [naFold_fst_concat] at h
  revert h; cases (naFold s).1 <;> cases (naFold s).2 <;> simp

-- snoc behaviour of the total
theorem selSum_concat_false {a : List ℤ} {s : List Bool} (h : a.length = s.length) (y : ℤ) :
    selSum (a ++ [y]) (s ++ [false]) = selSum a s := by
  unfold selSum; rw [List.zipWith_append h]; simp

theorem selSum_concat_true {a : List ℤ} {s : List Bool} (h : a.length = s.length) (y : ℤ) :
    selSum (a ++ [y]) (s ++ [true]) = selSum a s + y := by
  unfold selSum; rw [List.zipWith_append h]; simp

theorem length_concat_iff {a : List ℤ} {s : List Bool} {y : ℤ} {b : Bool} :
    (s ++ [b]).length = (a ++ [y]).length ↔ s.length = a.length := by
  simp [List.length_append]

/-- Base: the empty prefix admits only the empty selection (total `0`). -/
theorem valid_nil : Valid [] 0 0 := by
  have hsel : Sel [] [] := ⟨rfl, by simp [NoAdj, naFold]⟩
  refine ⟨⟨[], hsel, rfl⟩, ?_, ⟨[], hsel, by simp [EndFalse, naFold], rfl⟩, ?_⟩
  · rintro v ⟨s, _, hsum⟩; simp [selSum] at hsum; omega
  · rintro v ⟨s, _, _, hsum⟩; simp [selSum] at hsum; omega

/-- The Kadane step preserves the invariant when one house `y` is appended. -/
theorem valid_step {p : List ℤ} {f g : ℤ} (y : ℤ) (h : Valid p f g) :
    Valid (p ++ [y]) (step (f, g) y).1 (step (f, g) y).2 := by
  obtain ⟨⟨sf, ⟨hsflen, hsfna⟩, hsfsum⟩, hfOpt, ⟨sg, ⟨hsglen, hsgna⟩, hsgef, hsgsum⟩, hgOpt⟩ := h
  have hf_ext : Rob (p ++ [y]) f := by
    refine ⟨sf ++ [false], ⟨by rw [length_concat_iff]; exact hsflen, noAdj_false hsfna⟩, ?_⟩
    rw [selSum_concat_false hsflen.symm]; exact hsfsum
  have hgy_ext : Rob (p ++ [y]) (g + y) := by
    refine ⟨sg ++ [true], ⟨by rw [length_concat_iff]; exact hsglen, noAdj_true hsgna hsgef⟩, ?_⟩
    rw [selSum_concat_true hsglen.symm, hsgsum]
  refine ⟨?_, ?_, ?_, ?_⟩
  · -- fAch
    rw [step]; simp only
    rcases max_choice f (g + y) with hc | hc <;> rw [hc]
    · exact hf_ext
    · exact hgy_ext
  · -- fOpt
    rintro v ⟨s', ⟨hlen, hna⟩, hsum⟩
    rcases List.eq_nil_or_concat s' with rfl | ⟨t, b, rfl⟩
    · simp [List.length_append] at hlen
    rw [List.concat_eq_append] at hlen hna hsum
    have htlen : t.length = p.length := by rw [length_concat_iff] at hlen; exact hlen
    rw [step]; simp only
    cases b with
    | false =>
      rw [selSum_concat_false htlen.symm] at hsum
      exact le_trans (hfOpt v ⟨t, ⟨htlen, noAdj_false_inv hna⟩, hsum⟩) (le_max_left _ _)
    | true =>
      rw [selSum_concat_true htlen.symm] at hsum
      obtain ⟨htna, htef⟩ := noAdj_true_inv hna
      have hle := hgOpt (selSum p t) ⟨t, ⟨htlen, htna⟩, htef, rfl⟩
      exact le_trans (by omega) (le_max_right _ _)
  · -- gAch : g' = f
    rw [step]; simp only
    refine ⟨sf ++ [false], ⟨by rw [length_concat_iff]; exact hsflen, noAdj_false hsfna⟩,
      endFalse_false _, ?_⟩
    rw [selSum_concat_false hsflen.symm]; exact hsfsum
  · -- gOpt
    rintro v ⟨s', ⟨hlen, hna⟩, hef, hsum⟩
    rcases List.eq_nil_or_concat s' with rfl | ⟨t, b, rfl⟩
    · simp [List.length_append] at hlen
    rw [List.concat_eq_append] at hlen hna hsum hef
    have htlen : t.length = p.length := by rw [length_concat_iff] at hlen; exact hlen
    have hb : b = false := by unfold EndFalse at hef; rw [naFold_snd_concat] at hef; exact hef
    subst hb
    rw [step]; simp only
    rw [selSum_concat_false htlen.symm] at hsum
    exact hfOpt v ⟨t, ⟨htlen, noAdj_false_inv hna⟩, hsum⟩

/-- Folding the step over the houses preserves the invariant. -/
theorem valid_fold (a : List ℤ) :
    Valid a (a.foldl step (0, 0)).1 (a.foldl step (0, 0)).2 := by
  suffices h : ∀ ys p f g, Valid p f g →
      Valid (p ++ ys) (ys.foldl step (f, g)).1 (ys.foldl step (f, g)).2 by
    have hx := h a [] 0 0 valid_nil
    simpa using hx
  intro ys
  induction ys with
  | nil => intro p f g hv; simpa using hv
  | cons y ys ih =>
    intro p f g hv
    have hstep := valid_step y hv
    have hrec := ih (p ++ [y]) (step (f, g) y).1 (step (f, g) y).2 hstep
    simpa [List.foldl_cons, List.append_assoc] using hrec

/-- SCHEME (dp): the answer is a single streaming fold (the House Robber recurrence). -/
theorem cls : IsFold (fun a : List ℤ =>
    a.foldl (fun st y => (max st.1 (st.2 + y), st.1)) ((0 : ℤ), (0 : ℤ))) :=
  ⟨_, _, fun _ => rfl⟩

/-- CORRECT: `sol` is the sum of a real valid (no two adjacent) selection (ACHIEVABLE) and an
    upper bound on every such selection (OPTIMAL) — i.e. the maximum non-adjacent subset sum. -/
theorem corr (a : List ℤ) : Rob a (sol a) ∧ ∀ v, Rob a v → v ≤ sol a := by
  have hv := valid_fold a
  exact ⟨hv.fAch, fun v hvr => hv.fOpt v hvr⟩

end LC.P0198
