import Lproofs.Schemes.Fold

/-! @lc 309 | name:Best Time to Buy and Sell Stock with Cooldown | scheme:dp | family:dp-linear |
    complexity:O(n) | source:https://leetcode.com/problems/best-time-to-buy-and-sell-stock-with-cooldown/

    Max profit with unlimited transactions and a one-day cooldown after each sale. The DP tracks the
    best profit ending in each of three states: holding, just-sold (cooldown), and rest (free to
    buy). Correctness is the gold-standard pair — ACHIEVABLE (the answer is a real strategy's
    profit) and OPTIMAL (no valid strategy beats it). A strategy is a state per day with valid
    transitions, starting free-to-buy. -/

namespace LC.P0309
open Interview.Patterns

inductive St where | hold | sold | rest
  deriving DecidableEq

/-- Profit of a day's transition `prev → cur` at price `q`: buy on rest→hold, sell on hold→sold. -/
def tp (q : ℤ) : St → St → ℤ
  | .rest, .hold => -q
  | .hold, .sold => q
  | _, _ => 0

/-- Valid state transitions (sold must cool down to rest; rest may buy; hold may sell or keep). -/
def vt : St → St → Bool
  | .hold, .hold => true
  | .hold, .sold => true
  | .sold, .rest => true
  | .rest, .rest => true
  | .rest, .hold => true
  | _, _ => false

/-- Profit of a state-sequence on prices, threading the previous state. -/
def pf : St → List ℤ → List St → ℤ
  | _, [], _ => 0
  | _, _, [] => 0
  | prev, q :: qs, s :: ss => tp q prev s + pf s qs ss

/-- Profit of strategy `ss` on prices `p` (starting from the implicit `rest` state). -/
def profit (p : List ℤ) (ss : List St) : ℤ := pf .rest p ss

/-- Valid sequence from the previous state. -/
def vseq : St → List St → Bool
  | _, [] => true
  | prev, s :: ss => vt prev s && vseq s ss

/-- A valid strategy: aligned with prices, valid transitions starting from `rest`. -/
def Strat (p : List ℤ) (ss : List St) : Prop := ss.length = p.length ∧ vseq .rest ss = true

/-- Two-state-machine invariant: best profit ending in hold / sold / rest. -/
structure Valid (p : List ℤ) (hV sV rV : ℤ) : Prop where
  hAch : ∃ ss, Strat p ss ∧ ss.getLastD .rest = .hold ∧ profit p ss = hV
  hOpt : ∀ ss, Strat p ss → ss.getLastD .rest = .hold → profit p ss ≤ hV
  sAch : ∃ ss, Strat p ss ∧ ss.getLastD .rest = .sold ∧ profit p ss = sV
  sOpt : ∀ ss, Strat p ss → ss.getLastD .rest = .sold → profit p ss ≤ sV
  rAch : ∃ ss, Strat p ss ∧ ss.getLastD .rest = .rest ∧ profit p ss = rV
  rOpt : ∀ ss, Strat p ss → ss.getLastD .rest = .rest → profit p ss ≤ rV

/-- DP step: new hold/sold/rest values from the previous ones at price `q`. -/
def step (st : ℤ × ℤ × ℤ) (q : ℤ) : ℤ × ℤ × ℤ :=
  (max st.1 (st.2.2 - q), st.1 + q, max st.2.2 st.2.1)

def sol : List ℤ → ℤ
  | [] => 0
  | [_] => 0
  | p0 :: p1 :: rest =>
    let base : ℤ × ℤ × ℤ := (max (-p0) (-p1), -p0 + p1, 0)
    let f := rest.foldl step base
    max f.2.1 f.2.2

theorem getLastD_concat' (l : List St) (a d : St) : (l ++ [a]).getLastD d = a := by
  rw [List.getLastD_eq_getLast?, List.getLast?_concat]; rfl

/-- Appending one day adds exactly that day's transition profit. -/
theorem pf_concat (prev : St) : ∀ {p : List ℤ} {ss : List St}, p.length = ss.length →
    ∀ (q : ℤ) (s : St), pf prev (p ++ [q]) (ss ++ [s]) = pf prev p ss + tp q (ss.getLastD prev) s
  | [], [], _, q, s => by simp [pf, List.getLastD]
  | q' :: p', s' :: ss', hlen, q, s => by
      simp only [List.cons_append, pf, List.getLastD_cons]
      rw [pf_concat s' (by simpa using hlen) q s]; ring
  | [], _ :: _, hlen, _, _ => by simp at hlen
  | _ :: _, [], hlen, _, _ => by simp at hlen

/-- Appending one state to a valid sequence. -/
theorem vseq_concat (prev : St) : ∀ (ss : List St) (s : St),
    vseq prev (ss ++ [s]) = (vseq prev ss && vt (ss.getLastD prev) s)
  | [], s => by simp [vseq, List.getLastD]
  | s' :: ss', s => by
      simp only [List.cons_append, vseq, List.getLastD_cons, vseq_concat s' ss' s]
      rw [Bool.and_assoc]

/-- Extend a valid strategy by one state with a valid transition. -/
theorem extend {p : List ℤ} (q : ℤ) {t : List St} (s : St) (ht : t.length = p.length)
    (hv : vseq .rest t = true) (htr : vt (t.getLastD .rest) s = true) :
    Strat (p ++ [q]) (t ++ [s]) := by
  refine ⟨by simp only [List.length_append, List.length_cons, List.length_nil]; omega, ?_⟩
  rw [vseq_concat, hv, Bool.true_and]; exact htr

/-- The DP step preserves the invariant when one day `q` is appended. -/
theorem valid_step {p : List ℤ} {hV sV rV : ℤ} (q : ℤ) (hv : Valid p hV sV rV) :
    Valid (p ++ [q]) (step (hV, sV, rV) q).1 (step (hV, sV, rV) q).2.1 (step (hV, sV, rV) q).2.2 := by
  obtain ⟨⟨hs, ⟨hsl, hsv⟩, hse, hsp⟩, hOpt, ⟨ss2, ⟨ss2l, ss2v⟩, ss2e, ss2p⟩, sOpt,
    ⟨rs, ⟨rsl, rsv⟩, rse, rsp⟩, rOpt⟩ := hv
  rw [step]; simp only
  -- decompose any appended strategy into prefix + last state
  have decomp : ∀ (u : List St), Strat (p ++ [q]) u → ∃ t s, u = t ++ [s] ∧
      t.length = p.length ∧ vseq .rest t = true ∧ vt (t.getLastD .rest) s = true := by
    rintro u ⟨ul, uv⟩
    obtain ⟨t, s, rfl⟩ := (List.eq_nil_or_concat u).resolve_left (by
      rintro rfl; simp [List.length_append] at ul)
    rw [List.concat_eq_append] at ul uv ⊢
    rw [vseq_concat] at uv
    simp only [Bool.and_eq_true] at uv
    exact ⟨t, s, rfl, by simp [List.length_append] at ul; omega, uv.1, uv.2⟩
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_⟩
  · -- hAch
    rcases max_choice hV (rV - q) with hc | hc <;> rw [hc]
    · refine ⟨hs ++ [.hold], extend q .hold hsl hsv (by rw [hse]; rfl), getLastD_concat' .., ?_⟩
      rw [profit, pf_concat _ hsl.symm, hse]; simp only [tp]; simp only [profit] at hsp; linarith
    · refine ⟨rs ++ [.hold], extend q .hold rsl rsv (by rw [rse]; rfl), getLastD_concat' .., ?_⟩
      rw [profit, pf_concat _ rsl.symm, rse]; simp only [tp]; simp only [profit] at rsp; linarith
  · -- hOpt
    rintro u hu hend
    obtain ⟨t, s, rfl, tl, tv, ttr⟩ := decomp u hu
    rw [getLastD_concat'] at hend; subst hend
    rw [profit, pf_concat _ tl.symm]
    cases hp : t.getLastD .rest with
    | hold => simp only [tp]; have := hOpt t ⟨tl, tv⟩ hp; simp only [profit] at this
              linarith [le_max_left hV (rV - q)]
    | rest => simp only [tp]; have := rOpt t ⟨tl, tv⟩ hp; simp only [profit] at this
              linarith [le_max_right hV (rV - q)]
    | sold => rw [hp] at ttr; simp [vt] at ttr
  · -- sAch
    refine ⟨hs ++ [.sold], extend q .sold hsl hsv (by rw [hse]; rfl), getLastD_concat' .., ?_⟩
    rw [profit, pf_concat _ hsl.symm, hse]; simp only [tp]; simp only [profit] at hsp; linarith
  · -- sOpt
    rintro u hu hend
    obtain ⟨t, s, rfl, tl, tv, ttr⟩ := decomp u hu
    rw [getLastD_concat'] at hend; subst hend
    rw [profit, pf_concat _ tl.symm]
    cases hp : t.getLastD .rest with
    | hold => simp only [tp]; have := hOpt t ⟨tl, tv⟩ hp; simp only [profit] at this; linarith
    | sold => rw [hp] at ttr; simp [vt] at ttr
    | rest => rw [hp] at ttr; simp [vt] at ttr
  · -- rAch
    rcases max_choice rV sV with hc | hc <;> rw [hc]
    · refine ⟨rs ++ [.rest], extend q .rest rsl rsv (by rw [rse]; rfl), getLastD_concat' .., ?_⟩
      rw [profit, pf_concat _ rsl.symm, rse]; simp only [tp]; simp only [profit] at rsp; linarith
    · refine ⟨ss2 ++ [.rest], extend q .rest ss2l ss2v (by rw [ss2e]; rfl), getLastD_concat' .., ?_⟩
      rw [profit, pf_concat _ ss2l.symm, ss2e]; simp only [tp]; simp only [profit] at ss2p; linarith
  · -- rOpt
    rintro u hu hend
    obtain ⟨t, s, rfl, tl, tv, ttr⟩ := decomp u hu
    rw [getLastD_concat'] at hend; subst hend
    rw [profit, pf_concat _ tl.symm]
    cases hp : t.getLastD .rest with
    | rest => simp only [tp]; have := rOpt t ⟨tl, tv⟩ hp; simp only [profit] at this
              linarith [le_max_left rV sV]
    | sold => simp only [tp]; have := sOpt t ⟨tl, tv⟩ hp; simp only [profit] at this
              linarith [le_max_right rV sV]
    | hold => rw [hp] at ttr; simp [vt] at ttr

/-- Base: a two-day prefix, where the sold state first becomes reachable. -/
theorem valid_base2 (p0 p1 : ℤ) : Valid [p0, p1] (max (-p0) (-p1)) (-p0 + p1) 0 := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_⟩
  · rcases max_choice (-p0) (-p1) with hc | hc
    · exact ⟨[.hold, .hold], ⟨rfl, rfl⟩, rfl, by simp [profit, pf, tp, hc]⟩
    · exact ⟨[.rest, .hold], ⟨rfl, rfl⟩, rfl, by simp [profit, pf, tp, hc]⟩
  · rintro u ⟨ul, uv⟩ hend
    obtain ⟨s0, s1, rfl⟩ := List.length_eq_two.mp ul
    revert uv hend
    cases s0 <;> cases s1 <;> simp [vseq, vt, profit, pf, tp, List.getLastD] <;>
      first | omega | (intro; omega) | (intro; intro; omega)
  · exact ⟨[.hold, .sold], ⟨rfl, rfl⟩, rfl, by simp [profit, pf, tp]⟩
  · rintro u ⟨ul, uv⟩ hend
    obtain ⟨s0, s1, rfl⟩ := List.length_eq_two.mp ul
    revert uv hend
    cases s0 <;> cases s1 <;> simp [vseq, vt, profit, pf, tp, List.getLastD]
  · exact ⟨[.rest, .rest], ⟨rfl, rfl⟩, rfl, by simp [profit, pf, tp]⟩
  · rintro u ⟨ul, uv⟩ hend
    obtain ⟨s0, s1, rfl⟩ := List.length_eq_two.mp ul
    revert uv hend
    cases s0 <;> cases s1 <;> simp [vseq, vt, profit, pf, tp, List.getLastD]

/-- Folding the step over the remaining days preserves the invariant. -/
theorem valid_fold (p0 p1 : ℤ) (rest : List ℤ) :
    Valid (p0 :: p1 :: rest) (rest.foldl step (max (-p0) (-p1), -p0 + p1, 0)).1
      (rest.foldl step (max (-p0) (-p1), -p0 + p1, 0)).2.1
      (rest.foldl step (max (-p0) (-p1), -p0 + p1, 0)).2.2 := by
  suffices h : ∀ (ys : List ℤ) p hV sV rV, Valid p hV sV rV →
      Valid (p ++ ys) (ys.foldl step (hV, sV, rV)).1 (ys.foldl step (hV, sV, rV)).2.1
        (ys.foldl step (hV, sV, rV)).2.2 by
    have hx := h rest [p0, p1] _ _ _ (valid_base2 p0 p1)
    simpa using hx
  intro ys
  induction ys with
  | nil => intro p hV sV rV hvld; simpa using hvld
  | cons y ys ih =>
    intro p hV sV rV hvld
    have hrec := ih (p ++ [y]) _ _ _ (valid_step y hvld)
    simpa [List.foldl_cons, List.append_assoc] using hrec

/-- SCHEME (dp): the answer is a single streaming fold (the three-state recurrence). -/
theorem cls : IsFold (fun ps : List ℤ =>
    ps.foldl (fun st q => (max st.1 (st.2.2 - q), st.1 + q, max st.2.2 st.2.1))
      ((0 : ℤ), (0 : ℤ), (0 : ℤ))) :=
  ⟨_, _, fun _ => rfl⟩

/-- CORRECT: for ≥ 2 days, `sol` is the profit of a real valid strategy ending not-holding
    (ACHIEVABLE) and an upper bound on every such strategy (OPTIMAL) — the maximum profit. -/
theorem corr (p0 p1 : ℤ) (rest : List ℤ) :
    (∃ ss, Strat (p0 :: p1 :: rest) ss ∧ ss.getLastD .rest ≠ .hold ∧
        profit (p0 :: p1 :: rest) ss = sol (p0 :: p1 :: rest)) ∧
    (∀ ss, Strat (p0 :: p1 :: rest) ss → ss.getLastD .rest ≠ .hold →
        profit (p0 :: p1 :: rest) ss ≤ sol (p0 :: p1 :: rest)) := by
  have hvld := valid_fold p0 p1 rest
  set f := rest.foldl step (max (-p0) (-p1), -p0 + p1, 0) with hf
  have hsol : sol (p0 :: p1 :: rest) = max f.2.1 f.2.2 := rfl
  rw [hsol]
  refine ⟨?_, ?_⟩
  · rcases max_choice f.2.1 f.2.2 with hc | hc <;> rw [hc]
    · obtain ⟨ss, hst, he, hp⟩ := hvld.sAch
      exact ⟨ss, hst, by rw [he]; decide, hp⟩
    · obtain ⟨ss, hst, he, hp⟩ := hvld.rAch
      exact ⟨ss, hst, by rw [he]; decide, hp⟩
  · intro ss hst hne
    cases he : ss.getLastD .rest with
    | hold => exact absurd he hne
    | sold => exact le_trans (hvld.sOpt ss hst he) (le_max_left _ _)
    | rest => exact le_trans (hvld.rOpt ss hst he) (le_max_right _ _)

end LC.P0309
