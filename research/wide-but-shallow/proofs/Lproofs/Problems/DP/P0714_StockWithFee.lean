import Lproofs.Schemes.Fold

/-! @lc 714 | name:Best Time to Buy and Sell Stock with Transaction Fee | scheme:dp |
    family:dp-linear | complexity:O(n) |
    source:https://leetcode.com/problems/best-time-to-buy-and-sell-stock-with-transaction-fee/

    Maximum profit with unlimited transactions, a fee charged per sale. The two-state DP tracks the
    best profit ending not-holding (`cash`) and holding (`hold`). Correctness is the gold-standard
    pair — ACHIEVABLE (the answer is a real trading strategy's profit) and OPTIMAL (no strategy
    beats it). A strategy is a `List Bool` (hold-at-end-of-day) aligned with the prices; its last
    bit being `false`/`true` means it ends not-holding/holding. -/

namespace LC.P0714
open Interview.Patterns
variable (fee : ℤ)

/-- Profit contribution of one day: buy (`-q`) on a not-holding→holding transition, sell
    (`q - fee`) on a holding→not-holding transition. -/
def transit (prev : Bool) (q : ℤ) (cur : Bool) : ℤ :=
  (if prev = false ∧ cur = true then -q else 0) + (if prev = true ∧ cur = false then q - fee else 0)

theorem transit_FF (q : ℤ) : transit fee false q false = 0 := by simp [transit]
theorem transit_FT (q : ℤ) : transit fee false q true = -q := by simp [transit]
theorem transit_TF (q : ℤ) : transit fee true q false = q - fee := by simp [transit]
theorem transit_TT (q : ℤ) : transit fee true q true = 0 := by simp [transit]

/-- Total profit of a strategy, threading the previous holding state. -/
def profitAux : Bool → List ℤ → List Bool → ℤ
  | _, [], _ => 0
  | _, _, [] => 0
  | prev, q :: qs, b :: bs => transit fee prev q b + profitAux b qs bs

/-- Profit of strategy `h` on prices `p` (starting not holding). -/
def profit (p : List ℤ) (h : List Bool) : ℤ := profitAux fee false p h

/-- Two-state invariant: `cashV` / `holdV` are the best profits ending not-holding / holding. -/
structure Valid (p : List ℤ) (cashV holdV : ℤ) : Prop where
  cashAch : ∃ h, h.length = p.length ∧ h.getLastD false = false ∧ profit fee p h = cashV
  cashOpt : ∀ h, h.length = p.length → h.getLastD false = false → profit fee p h ≤ cashV
  holdAch : ∃ h, h.length = p.length ∧ h.getLastD false = true ∧ profit fee p h = holdV
  holdOpt : ∀ h, h.length = p.length → h.getLastD false = true → profit fee p h ≤ holdV

def step (st : ℤ × ℤ) (q : ℤ) : ℤ × ℤ := (max st.1 (st.2 + q - fee), max st.2 (st.1 - q))

def sol : List ℤ → ℤ
  | [] => 0
  | p0 :: rest => (rest.foldl (step fee) (0, -p0)).1

theorem getLastD_concat' (l : List Bool) (a d : Bool) : (l ++ [a]).getLastD d = a := by
  rw [List.getLastD_eq_getLast?, List.getLast?_concat]; rfl

theorem len_concat {α β} (l : List α) (a : α) (m : List β) (b : β) :
    (l ++ [a]).length = (m ++ [b]).length ↔ l.length = m.length := by
  simp [List.length_append]

/-- Appending one day to a strategy adds exactly that day's transition contribution. -/
theorem profitAux_concat (prev : Bool) : ∀ {p : List ℤ} {h : List Bool}, p.length = h.length →
    ∀ (q : ℤ) (b : Bool),
      profitAux fee prev (p ++ [q]) (h ++ [b]) =
        profitAux fee prev p h + transit fee (h.getLastD prev) q b
  | [], [], _, q, b => by simp [profitAux, List.getLastD]
  | q' :: p', b' :: h', hlen, q, b => by
      simp only [List.cons_append, profitAux, List.getLastD_cons]
      rw [profitAux_concat b' (by simpa using hlen) q b]; ring
  | [], _ :: _, hlen, _, _ => by simp at hlen
  | _ :: _, [], hlen, _, _ => by simp at hlen

theorem profit_concat {p : List ℤ} {h : List Bool} (hlen : p.length = h.length) (q : ℤ) (b : Bool) :
    profit fee (p ++ [q]) (h ++ [b]) = profit fee p h + transit fee (h.getLastD false) q b :=
  profitAux_concat fee false hlen q b

/-- Base: a one-day prefix — do nothing (profit 0, not holding) or buy (profit `-p0`, holding). -/
theorem valid_single (p0 : ℤ) : Valid fee [p0] 0 (-p0) := by
  refine ⟨⟨[false], rfl, rfl, by simp [profit, profitAux, transit]⟩, ?_,
    ⟨[true], rfl, rfl, by simp [profit, profitAux, transit]⟩, ?_⟩
  · rintro h hlen hef
    match h with
    | [b] =>
      have hb : b = false := by simpa [List.getLastD] using hef
      subst hb; simp [profit, profitAux, transit]
  · rintro h hlen het
    match h with
    | [b] =>
      have hb : b = true := by simpa [List.getLastD] using het
      subst hb; simp [profit, profitAux, transit]

/-- The step preserves the invariant when one day `q` is appended. -/
theorem valid_step {p : List ℤ} {cashV holdV : ℤ} (q : ℤ) (hv : Valid fee p cashV holdV) :
    Valid fee (p ++ [q]) (step fee (cashV, holdV) q).1 (step fee (cashV, holdV) q).2 := by
  obtain ⟨⟨cf, hcf, hcef, hcfp⟩, hcOpt, ⟨hf, hhf, hhef, hhfp⟩, hhOpt⟩ := hv
  rw [step]; simp only
  refine ⟨?_, ?_, ?_, ?_⟩
  · rcases max_choice cashV (holdV + q - fee) with hc | hc <;> rw [hc]
    · exact ⟨cf ++ [false], (len_concat ..).mpr hcf, getLastD_concat' .., by
        rw [profit_concat fee hcf.symm, hcef, transit_FF, add_zero]; exact hcfp⟩
    · exact ⟨hf ++ [false], (len_concat ..).mpr hhf, getLastD_concat' .., by
        rw [profit_concat fee hhf.symm, hhef, transit_TF, hhfp]; ring⟩
  · rintro h hlen hef
    obtain ⟨t, b, rfl⟩ := (List.eq_nil_or_concat h).resolve_left (by
      rintro rfl; simp [List.length_append] at hlen)
    rw [List.concat_eq_append] at hlen hef ⊢
    have htlen : p.length = t.length := by rw [len_concat] at hlen; omega
    have hb : b = false := by rw [getLastD_concat'] at hef; exact hef
    subst hb
    rw [profit_concat fee htlen]
    rcases eq_or_ne (t.getLastD false) false with ht | ht
    · rw [ht, transit_FF, add_zero]; exact le_trans (hcOpt t htlen.symm ht) (le_max_left _ _)
    · have ht' : t.getLastD false = true := by simpa [Bool.not_eq_false] using ht
      rw [ht', transit_TF]; have := hhOpt t htlen.symm ht'; exact le_trans (by linarith) (le_max_right _ _)
  · rcases max_choice holdV (cashV - q) with hc | hc <;> rw [hc]
    · exact ⟨hf ++ [true], (len_concat ..).mpr hhf, getLastD_concat' .., by
        rw [profit_concat fee hhf.symm, hhef, transit_TT, add_zero]; exact hhfp⟩
    · exact ⟨cf ++ [true], (len_concat ..).mpr hcf, getLastD_concat' .., by
        rw [profit_concat fee hcf.symm, hcef, transit_FT]; linarith [hcfp]⟩
  · rintro h hlen het
    obtain ⟨t, b, rfl⟩ := (List.eq_nil_or_concat h).resolve_left (by
      rintro rfl; simp [List.length_append] at hlen)
    rw [List.concat_eq_append] at hlen het ⊢
    have htlen : p.length = t.length := by rw [len_concat] at hlen; omega
    have hb : b = true := by rw [getLastD_concat'] at het; exact het
    subst hb
    rw [profit_concat fee htlen]
    rcases eq_or_ne (t.getLastD false) false with ht | ht
    · rw [ht, transit_FT]; have := hcOpt t htlen.symm ht; exact le_trans (by linarith) (le_max_right _ _)
    · have ht' : t.getLastD false = true := by simpa [Bool.not_eq_false] using ht
      rw [ht', transit_TT, add_zero]; exact le_trans (hhOpt t htlen.symm ht') (le_max_left _ _)

/-- Folding the step over the remaining days preserves the invariant. -/
theorem valid_fold (p0 : ℤ) (rest : List ℤ) :
    Valid fee (p0 :: rest) (rest.foldl (step fee) (0, -p0)).1 (rest.foldl (step fee) (0, -p0)).2 := by
  suffices h : ∀ (ys : List ℤ) p cashV holdV, Valid fee p cashV holdV →
      Valid fee (p ++ ys) (ys.foldl (step fee) (cashV, holdV)).1
        (ys.foldl (step fee) (cashV, holdV)).2 by
    have hx := h rest [p0] 0 (-p0) (valid_single fee p0)
    simpa using hx
  intro ys
  induction ys with
  | nil => intro p cashV holdV hv; simpa using hv
  | cons y ys ih =>
    intro p cashV holdV hv
    have hrec := ih (p ++ [y]) (step fee (cashV, holdV) y).1 (step fee (cashV, holdV) y).2
      (valid_step fee y hv)
    simpa [List.foldl_cons, List.append_assoc] using hrec

/-- SCHEME (dp): the answer is a single streaming fold (the cash/hold recurrence). -/
theorem cls : IsFold (fun ps : List ℤ =>
    ps.foldl (fun st q => (max st.1 (st.2 + q - fee), max st.2 (st.1 - q))) ((0 : ℤ), (0 : ℤ))) :=
  ⟨_, _, fun _ => rfl⟩

/-- CORRECT: for a nonempty price list, `sol` is the profit of a real not-holding-ending strategy
    (ACHIEVABLE) and an upper bound on every such strategy (OPTIMAL) — the maximum profit. -/
theorem corr (p0 : ℤ) (rest : List ℤ) :
    (∃ h, h.length = (p0 :: rest).length ∧ h.getLastD false = false ∧
        profit fee (p0 :: rest) h = sol fee (p0 :: rest)) ∧
    (∀ h, h.length = (p0 :: rest).length → h.getLastD false = false →
        profit fee (p0 :: rest) h ≤ sol fee (p0 :: rest)) := by
  have hv := valid_fold fee p0 rest
  exact ⟨hv.cashAch, fun h hs he => hv.cashOpt h hs he⟩

end LC.P0714
